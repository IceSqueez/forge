use std::borrow::Cow;

pub fn embedded_fonts() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Italic.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/Inter-SemiBold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf").as_slice()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE_COUNT_AT: usize = 4;
    const TABLE_RECORDS_AT: usize = 12;
    const TABLE_RECORD_LEN: usize = 16;
    const WEIGHT_CLASS_AT: usize = 4;
    const FS_SELECTION_AT: usize = 62;
    const ITALIC_BIT: u16 = 1;
    const REGULAR_WEIGHT: u16 = 400;

    fn be_u16(face: &[u8], at: usize) -> u16 {
        u16::from_be_bytes([face[at], face[at + 1]])
    }

    fn be_u32(face: &[u8], at: usize) -> usize {
        u32::from_be_bytes([face[at], face[at + 1], face[at + 2], face[at + 3]]) as usize
    }

    fn os2_table(face: &[u8]) -> Option<usize> {
        (0..usize::from(be_u16(face, TABLE_COUNT_AT)))
            .map(|index| TABLE_RECORDS_AT + index * TABLE_RECORD_LEN)
            .find(|record| &face[*record..*record + 4] == b"OS/2")
            .map(|record| be_u32(face, record + 8))
    }

    fn weight_and_italic(face: &[u8]) -> Option<(u16, bool)> {
        let os2 = os2_table(face)?;
        let italic = be_u16(face, os2 + FS_SELECTION_AT) & ITALIC_BIT != 0;
        Some((be_u16(face, os2 + WEIGHT_CLASS_AT), italic))
    }

    #[test]
    fn the_embedded_faces_include_a_regular_weight_italic_for_placeholders() {
        let styles: Vec<Option<(u16, bool)>> = embedded_fonts()
            .iter()
            .map(|face| weight_and_italic(face))
            .collect();

        assert!(
            styles.contains(&Some((REGULAR_WEIGHT, true))),
            "no regular italic face among {styles:?}"
        );
    }
}
