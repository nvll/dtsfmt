use crate::config::Config;

// Devicetree Specification section 2.3.5 #address-cells and #size-cells
pub const ADDRESS_CELLS_DEFAULT: u32 = 2;
pub const SIZE_CELLS_DEFAULT: u32 = 1;

#[derive(Debug)]
pub struct Context<'a, 'b> {
    pub indent: usize,
    pub keymap: bool,
    pub bindings: bool,
    // The nearest identifier found while descending through the tree.
    pub identifier: Option<&'b str>,
    // The nearest #address-cells specification found descending through the
    // tree.
    pub address_cells: u32,
    // The nearest #size-cells specification found descending through the tree.
    pub size_cells: u32,
    pub config: &'a Config,
}

impl<'a, 'b> Context<'a, 'b> {
    pub fn inc(&self, increment: usize) -> Self {
        Self { indent: self.indent + increment, ..*self }
    }

    pub fn dec(&self, decrement: usize) -> Self {
        Self { indent: self.indent - decrement, ..*self }
    }

    pub fn keymap(&self) -> Self {
        Self { keymap: true, ..*self }
    }

    pub fn bindings(&self) -> Self {
        Self { bindings: true, ..*self }
    }

    // If a node named 'bindings' has a parent node named 'keymap' then we've
    // encountered a Zephyr keymap that will be handled as a special case by the
    // printer.
    pub fn has_zephyr_syntax(&self) -> bool {
        self.bindings && self.keymap
    }

    pub fn with_identifier(&'a self, v: &'b str) -> Self {
        Self { identifier: Some(v), ..*self }
    }
    pub fn with_address_cells(&self, v: u32) -> Self {
        Self { address_cells: v, ..*self }
    }

    pub fn with_size_cells(&self, v: u32) -> Self {
        Self { size_cells: v, ..*self }
    }
}
