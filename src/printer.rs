use std::collections::VecDeque;

use tree_sitter::{Node, TreeCursor};

use crate::config::Config;
use crate::context::{self, Context};
use crate::layouts;
use crate::parser::parse;
use crate::utils::{
    get_text, lookahead, lookbehind, pad_right, print_indent, sep,
};

fn is_preproc(n: &tree_sitter::Node) -> bool {
    n.kind() == "preproc_include"
        || n.kind() == "preproc_ifdef"
        || n.kind() == "preproc_def"
        || n.kind() == "preproc_function_def"
}

fn extract_cells_value(source: &String, node: &Node) -> Option<u32> {
    // A property with a single integer literal will have the format of:
    // 'identifier>' = 'integer_cells'
    //                         → < 'integer_literal' >
    // Conveniently, this means we know exactly which named child contains the value needed.
    let s = get_text(
        source,
        &mut node.named_child(1).unwrap().named_child(0).unwrap().walk(),
    );

    // Per spec, the value can be either hex or an int.
    if s.starts_with("0x") {
        u32::from_str_radix(s.strip_prefix("0x").unwrap(), 16).ok()
    } else {
        u32::from_str_radix(s, 10).ok()
    }
}

fn parse_address_and_size_cells<'a, 'b>(
    source: &String,
    node: &Node,
) -> (Option<u32>, Option<u32>) {
    let mut cursor = node.walk();
    let mut address_cells = None;
    let mut size_cells = None;

    let properties: Vec<Node> = node
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "property")
        .collect();
    for property in properties {
        let identifier =
            get_text(source, &mut property.named_child(0).unwrap().walk());
        match identifier {
            "#address-cells" => {
                address_cells = extract_cells_value(source, &property)
            }
            "#size-cells" => {
                size_cells = extract_cells_value(source, &property)
            }
            _ => (),
        }
    }
    (address_cells, size_cells)
}

fn print_int_strs(
    writer: &mut String,
    int_strs: &[&str],
    chunk_size: u32,
    ctx: &Context,
) {
    let chunks: Vec<&[&str]> = int_strs.chunks(chunk_size as usize).collect();
    // For smaller byte chunks it reads better if we just one line
    // everything, but for anything beyond 16 bytes we split it into
    // multiple lines.
    if chunks.len() == 1 {
        writer.push_str(&chunks[0].join(" "));
    } else {
        for (i, &chunk) in chunks.iter().enumerate() {
            if i != 0 {
                print_indent(writer, &ctx);
                // This aligns us to just after the initial [ or < based on the length of the identifier and " ="
                writer.push_str(
                    &" ".repeat(ctx.identifier.unwrap_or("").len() + 2),
                );
            }
            writer.push_str(&chunk.join(" "));
            if i != chunks.len() - 1 {
                writer.push('\n');
            }
        }
    }
}

fn traverse(
    writer: &mut String,
    source: &String,
    cursor: &mut TreeCursor,
    ctx: &Context,
) {
    let node = cursor.node();

    match node.kind() {
        "file_version" => {
            writer.push_str(&format!("{}\n\n", get_text(source, cursor)));
        }
        "comment" => {
            // Add a newline before the comment if the previous node is not a
            // comment
            if lookbehind(cursor).is_some_and(|n| n.kind() != "comment") {
                sep(writer);
            }

            print_indent(writer, ctx);
            let comment = get_text(source, cursor);

            // Only reformat single line comments, multi line comments are a
            // lot tougher to format properly.
            match comment.starts_with("//") {
                true => {
                    writer.push_str("// ");
                    writer.push_str(comment.trim_start_matches("//").trim());
                }
                false => writer.push_str(comment),
            }

            writer.push('\n');
        }
        "dtsi_include" => {
            cursor.goto_first_child();
            print_indent(writer, ctx);
            writer.push_str("/include/ ");

            cursor.goto_next_sibling();
            writer.push_str(get_text(source, cursor));
            writer.push('\n');

            cursor.goto_parent();

            // Add a newline if this is the last dtsi_include
            if lookahead(cursor).is_some_and(|n| n.kind() != "dtsi_include") {
                writer.push('\n');
            }
        }
        "preproc_include" => {
            cursor.goto_first_child();
            print_indent(writer, ctx);
            writer.push_str("#include ");

            cursor.goto_next_sibling();
            writer.push_str(get_text(source, cursor));
            writer.push('\n');

            cursor.goto_parent();

            // Add a newline if this is the last preproc directive
            if lookahead(cursor).is_some_and(|n| !is_preproc(&n)) {
                writer.push('\n');
            }
        }
        "preproc_def" => {
            cursor.goto_first_child();
            writer.push_str("#define ");

            // Name
            cursor.goto_next_sibling();
            writer.push_str(get_text(source, cursor));
            writer.push(' ');

            // Value
            cursor.goto_next_sibling();
            writer.push_str(get_text(source, cursor));

            writer.push('\n');
            cursor.goto_parent();

            // Add a newline if this is the last preproc directive
            if lookahead(cursor).is_some_and(|n| !is_preproc(&n)) {
                writer.push('\n');
            }
        }
        "preproc_function_def" => {
            cursor.goto_first_child();
            writer.push_str("#define ");

            // Function and args
            for _ in 0..2 {
                cursor.goto_next_sibling();
                writer.push_str(get_text(source, cursor));
            }
            writer.push(' ');

            // Value
            cursor.goto_next_sibling();
            writer.push_str(get_text(source, cursor));

            writer.push('\n');
            cursor.goto_parent();

            // Add a newline if this is the last preproc directive
            if lookahead(cursor).is_some_and(|n| !is_preproc(&n)) {
                writer.push('\n');
            }
        }
        "preproc_ifdef" => {
            print_indent(writer, ctx);

            // #ifdef
            cursor.goto_first_child();
            writer.push_str(get_text(source, cursor).trim());
            writer.push(' ');

            // Name
            cursor.goto_next_sibling();
            writer.push_str(get_text(source, cursor));
            writer.push('\n');

            // Body
            while cursor.goto_next_sibling() {
                traverse(writer, source, cursor, ctx);
            }

            // Closing
            print_indent(writer, ctx);
            writer.push_str("#endif\n");
            cursor.goto_parent();

            // Add a newline if this is the last preproc directive
            if lookahead(cursor).is_some_and(|n| !is_preproc(&n)) {
                writer.push('\n');
            }
        }
        "identifier" | "integer_literal" | "string_literal"
        | "unit_address" => {
            writer.push_str(get_text(source, cursor));
        }
        // Node and Property are handled togetherb due to shared handling of
        // identifiers and indentation of children.
        "node" | "property" => {
            // Check for any #address-cells or #size-cells properties in this
            // mode before descending since they affect child parsing.
            let (address_cells, size_cells) =
                parse_address_and_size_cells(source, &node);

            // A node will typically have children in a format of:
            // [<identifier>:] [&]<identifier> { [nodes and properties] }
            cursor.goto_first_child();

            // Nodes are preceded by a label or name identifier that need to be
            // indented. We can check for this by seeing if any siblings are
            // before us.
            if cursor.node().prev_sibling().is_none() {
                print_indent(writer, ctx);
            }

            // Update the context before we traverse children. We always need to
            // indent once per level, and then if we found a more local
            // #address-cells or #size-cells value we want to carry it with us
            // because it takes effect for children of the node they are present
            // in.
            let identifier = get_text(source, cursor);
            let ctx = ctx
                .with_identifier(identifier)
                .with_address_cells(address_cells.unwrap_or(ctx.address_cells))
                .with_size_cells(size_cells.unwrap_or(ctx.size_cells))
                .inc(1);
            // Update whether we've seen Zephyr-shaped identifiers.
            let ctx = match identifier {
                "keymap" => ctx.keymap(),
                "bindings" => ctx.bindings(),
                _ => ctx,
            };

            loop {
                traverse(writer, source, cursor, &ctx);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }

            // Return to the "node"'s node element to continue traversal.
            cursor.goto_parent();

            // Place a newline before node siblings if they follow a property.
            if node.kind() == "property"
                && lookahead(cursor).is_some_and(|n| n.kind() == "node")
            {
                writer.push('\n');
            }
        }
        "byte_string_literal" => {
            let hex_string = get_text(source, cursor);
            // Trim the [ and ] off of the source string we obtained.
            let children = hex_string[1..hex_string.len() - 1]
                .split_whitespace()
                .collect::<Vec<&str>>();
            writer.push('[');
            print_int_strs(writer, &children, 16, ctx);
            writer.push(']');
        }

        "integer_cells" => {
            let chunk_size = match &ctx.identifier {
                Some("reg") => ctx.address_cells + ctx.size_cells,
                _ => 16,
            };

            // Keymap bindings are a special snowflake
            if ctx.has_zephyr_syntax() {
                cursor.goto_first_child();
                print_bindings(writer, source, cursor, ctx);
                return;
            }

            let children: Vec<&str> = node
                .named_children(&mut node.walk())
                .map(|n| get_text(source, &mut n.walk()))
                .collect();
            writer.push('<');
            print_int_strs(writer, &children, chunk_size, &ctx);
            writer.push('>');
        }
        // All the non-named grammatical tokens that are emitted but handled
        // simply with some output structure.
        "@" => {
            writer.push('@');
        }
        "}" => {
            print_indent(writer, &ctx.dec(1));
            writer.push('}');
        }
        "{" => {
            writer.push_str(" {\n");
        }
        ":" => {
            writer.push_str(": ");
        }
        ";" => {
            writer.push_str(";\n");
        }
        "," => {
            writer.push_str(", ");
        }
        "=" => {
            writer.push_str(" = ");
        }
        _ => {
            if ctx.config.warn_on_unhandled_tokens {
                eprintln!(
                    "unhandled type '{}' ({} {}): {}",
                    node.kind(),
                    node.child_count(),
                    if node.child_count() == 1 { "child" } else { "children" },
                    get_text(source, cursor)
                );
            }
            // Since we're unsure of this node just traverse its children
            if cursor.goto_first_child() {
                traverse(writer, source, cursor, ctx);

                while cursor.goto_next_sibling() {
                    traverse(writer, source, cursor, &ctx.inc(1));
                }

                cursor.goto_parent();
            }
        }
    };
}

fn collect_bindings(
    cursor: &mut TreeCursor,
    source: &String,
    ctx: &Context,
) -> VecDeque<String> {
    let mut buf: VecDeque<String> = VecDeque::new();
    let mut item = String::new();

    while cursor.goto_next_sibling() {
        match cursor.node().kind() {
            ">" => break,
            _ => {
                let text = get_text(source, cursor).trim();

                // If this is a new binding, add a new item to the buffer
                if !item.is_empty() && text.starts_with("&") {
                    buf.push_back(item);
                    item = String::new();
                }

                // Add a space between each piece of text
                if !item.is_empty() {
                    item.push(' ');
                }

                // Add the current piece of text to the buffer
                item.push_str(text);
            }
        }
    }

    // Add the last item to the buffer
    buf.push_back(item);

    // Move the items from the temporary buffer into a new vector that contains
    // the empty key spaces.
    layouts::get_layout(&ctx.config.layout)
        .bindings
        .iter()
        .map(|is_key| match is_key {
            1 => buf.pop_front().unwrap_or_default(),
            _ => String::new(),
        })
        .collect()
}

/// Calculate the maximum size of each column in the bindings table.
fn calculate_sizes(buf: &VecDeque<String>, row_size: usize) -> Vec<usize> {
    let mut sizes = Vec::new();

    for i in 0..row_size {
        let mut max = 0;

        for j in (i..buf.len()).step_by(row_size) {
            let len = buf[j].len();

            if len > max {
                max = len;
            }
        }

        sizes.push(max);
    }

    sizes
}

fn print_bindings(
    writer: &mut String,
    source: &String,
    cursor: &mut TreeCursor,
    ctx: &Context,
) {
    cursor.goto_first_child();
    writer.push('<');

    let buf = collect_bindings(cursor, source, ctx);
    let row_size = layouts::get_layout(&ctx.config.layout).row_size();
    let sizes = calculate_sizes(&buf, row_size);

    buf.iter().enumerate().for_each(|(i, item)| {
        let col = i % row_size;

        // Add a newline at the start of each row
        if col == 0 {
            writer.push('\n');
            print_indent(writer, ctx);
        }

        // Don't add padding to the last binding in the row
        let padding = match (i + 1) % row_size == 0 {
            true => 0,
            false => sizes[col] + 3,
        };

        writer.push_str(&pad_right(item, padding));
    });

    // Close the bindings
    writer.push('\n');
    print_indent(writer, &ctx.dec(1));
    writer.push('>');

    cursor.goto_parent();
}

pub fn print(source: &String, config: &Config) -> String {
    let mut writer = String::new();
    let tree = parse(source.clone());
    let mut cursor = tree.walk();

    let ctx = Context {
        indent: 0,
        keymap: false,
        bindings: false,
        identifier: None,
        address_cells: context::ADDRESS_CELLS_DEFAULT,
        size_cells: context::SIZE_CELLS_DEFAULT,
        config,
    };

    // The first node is the root document node, so we have to traverse all it's
    // children with the same indentation level.
    cursor.goto_first_child();
    traverse(&mut writer, source, &mut cursor, &ctx);

    while cursor.goto_next_sibling() {
        traverse(&mut writer, source, &mut cursor, &ctx);
    }

    writer
}
