use forge_types::VariantKind;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubActionIo {
    pub produces: Vec<ProducedVariable>,
    pub consumes: Vec<ConsumedVariable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducedVariable {
    /// Config key whose author-supplied value names the produced scope variable.
    pub output_name_key: String,
    pub kind: VariantKind,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumedVariable {
    pub name: String,
    pub kind: VariantKind,
}
