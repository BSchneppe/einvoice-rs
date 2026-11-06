// ============================================================================
// Skip Frame (Error Recovery)
// ============================================================================

/// Frame for skipping unknown or invalid elements
pub struct SkipFrame {
    depth: u32,
    source: Option<SourceRef>,
}

impl SkipFrame {
    pub fn new(source: Option<SourceRef>) -> Self {
        Self { depth: 0, source }
    }

    /// Increment depth when entering a child
    pub fn enter(&mut self) {
        self.depth += 1;
    }

    /// Decrement depth when leaving a child
    pub fn leave(&mut self) -> bool {
        if self.depth > 0 {
            self.depth -= 1;
            false
        } else {
            true
        }
    }
}

impl Frame for SkipFrame {
    fn allows(&self, _local_name: &str, _name_table: &NameTable) -> bool {
        true // Accept everything when skipping
    }

    fn allows_attribute(&self, _local_name: &str, _name_table: &NameTable) -> bool {
        true
    }

    fn on_child_start(&mut self, _local_name: &str, _name_table: &NameTable) {
        self.enter();
    }

    fn attach(&mut self, _child: FrameResult) -> SchemaResult<()> {
        Ok(())
    }

    fn finish(self: Box<Self>) -> SchemaResult<FrameResult> {
        Ok(FrameResult::Skip)
    }

    fn source(&self) -> Option<&SourceRef> {
        self.source.as_ref()
    }

    fn set_foreign_attributes(&mut self, _attrs: Vec<ForeignAttribute>) {}

    fn is_skip_frame(&self) -> bool {
        true
    }

    fn on_child_end(&mut self) -> bool {
        self.leave()
    }
}

