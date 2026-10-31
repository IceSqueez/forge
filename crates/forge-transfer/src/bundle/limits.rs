const KIB: u64 = 1024;
const MIB: u64 = KIB * KIB;
const GIB: u64 = MIB * KIB;

pub(crate) const MAX_PUBLIC_MANIFEST_BYTES: u64 = 64 * KIB;
pub(crate) const MAX_FULL_MANIFEST_BYTES: u64 = 64 * MIB;
pub(crate) const MAX_PACK_MANIFEST_BYTES: u64 = 16 * MIB;
pub(crate) const MAX_PACK_DOCUMENTS_BYTES: u64 = 64 * MIB;
pub(crate) const MAX_ENTRY_BYTES: u64 = 16 * GIB;
pub(crate) const MAX_UNPACKED_BYTES: u64 = 256 * GIB;
pub(crate) const MAX_ARCHIVE_ENTRIES: usize = 1_000_000;
pub(crate) const MAX_SHARED_ENTRY_BYTES: u64 = GIB;
pub(crate) const MAX_SHARED_UNPACKED_BYTES: u64 = 4 * GIB;
pub(crate) const MAX_SHARED_ARCHIVE_ENTRIES: usize = 100_000;
pub(crate) const ENCRYPT_SCRYPT_WORK_FACTOR: u8 = 18;
pub const MAX_DECRYPT_SCRYPT_WORK_FACTOR: u8 = 20;
