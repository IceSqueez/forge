pub trait ConfigChoice: Copy + Sync + 'static {
    const ALL: &'static [Self];
    const WIRE_NAMES: &'static [&'static str];

    fn wire_name(self) -> &'static str;

    fn parse(raw: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|choice| choice.wire_name() == raw)
    }
}
