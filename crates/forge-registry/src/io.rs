use forge_types::VariantKind;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubActionIo {
    pub produces: Vec<ProducedVariable>,
    pub fixed_outputs: Vec<String>,
    pub body_locals: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProducedVariable {
    pub output_name_key: String,
    pub kind: VariantKind,
    pub label: String,
}
