//! The frozen TIV 1.0 codes (`dmt_types.h`) for the schema's enums, contract versions, and the
//! well-known role table (R-WK).

use super::schema::{KeyType, Kind, Persist, PrincipalFlag, Role};

/// `DMT_KF_*` key flags.
pub const KF_SECRET: u16 = 0x0001;
pub const KF_SURVIVES_RESET: u16 = 0x0002;
pub const KF_ALIAS: u16 = 0x0004;
pub const KF_ENUM: u16 = 0x0008;
pub const KF_CMD: u16 = 0x0010;
pub const KF_CMD_RESULT: u16 = 0x0020;
pub const KF_CMD_DEDUPE: u16 = 0x0040;
pub const KF_MFG: u16 = 0x0080;

/// `DMT_X_*` transport mask bits.
pub const X_UNIX: u8 = 0x01;
pub const X_INPROC: u8 = 0x02;
pub const X_UART: u8 = 0x04;

/// `DMT_*_HEADER_BYTES` and `DMT_DEDUPE_ENTRY_BYTES`.
pub const CMD_HEADER_BYTES: u32 = 8;
pub const RESULT_HEADER_BYTES: u32 = 12;
pub const DEDUPE_ENTRY_BYTES: u32 = 64;

/// The command owner that means "the core answers it"; reserved, never a principal name.
pub const OWNER_CORE: &str = "core";
/// `DMT_OWNER_CORE`, its `owner_principal` value.
pub const OWNER_CORE_INDEX: u16 = 0xFFFF;

/// A contract version within which minors are additive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
}

impl Version {
    /// Parse `"major.minor"`.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-002
    pub fn parse(text: &str) -> Result<Self, String> {
        let parsed = text
            .split_once('.')
            .and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?)));
        match parsed {
            Some((major, minor)) => Ok(Self { major, minor }),
            None => Err(format!("'{text}' is not a \"major.minor\" version")),
        }
    }

    /// The minor as seen from contract major `major`: this minor in the same major, 0 for an
    /// older major (a major bump restarts the count).
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn minor_in(self, major: u16) -> u16 {
        if self.major == major {
            self.minor
        } else {
            0
        }
    }
}

impl std::fmt::Display for Version {
    /// Format as `"major.minor"`.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-002
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl KeyType {
    /// `DMT_T_*` code; also bits 6..3 of the key ID.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn code(self) -> u8 {
        match self {
            KeyType::Bool => 0,
            KeyType::Uint8 => 1,
            KeyType::Uint16 => 2,
            KeyType::Uint32 => 3,
            KeyType::Int8 => 4,
            KeyType::Int16 => 5,
            KeyType::Int32 => 6,
            KeyType::Float32 => 7,
            KeyType::String => 8,
            KeyType::Binary => 9,
            KeyType::Struct => 10,
        }
    }

    /// Size in bytes of a scalar, `None` for string, binary and struct (sized by max_size).
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn scalar_size(self) -> Option<u32> {
        match self {
            KeyType::Bool | KeyType::Uint8 | KeyType::Int8 => Some(1),
            KeyType::Uint16 | KeyType::Int16 => Some(2),
            KeyType::Uint32 | KeyType::Int32 | KeyType::Float32 => Some(4),
            KeyType::String | KeyType::Binary | KeyType::Struct => None,
        }
    }

    /// Name used in the model and in contract.json.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-009
    pub fn name(self) -> &'static str {
        match self {
            KeyType::Bool => "bool",
            KeyType::Uint8 => "uint8",
            KeyType::Int8 => "int8",
            KeyType::Uint16 => "uint16",
            KeyType::Int16 => "int16",
            KeyType::Uint32 => "uint32",
            KeyType::Int32 => "int32",
            KeyType::Float32 => "float32",
            KeyType::String => "string",
            KeyType::Binary => "binary",
            KeyType::Struct => "struct",
        }
    }
}

impl Persist {
    /// Name used in the model, contract.json and refusal messages.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-009
    pub fn name(self) -> &'static str {
        match self {
            Persist::None => "none",
            Persist::Deferred => "deferred",
            Persist::Durable => "durable",
        }
    }

    /// `DMT_P_*` code.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn code(self) -> u8 {
        match self {
            Persist::None => 0,
            Persist::Deferred => 1,
            Persist::Durable => 2,
        }
    }
}

impl Role {
    /// `DMT_R_*` code.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-003
    pub fn code(self) -> u8 {
        match self {
            Role::Settings => 0,
            Role::State => 1,
            Role::Command => 2,
            Role::Config => 3,
            Role::Diag => 4,
        }
    }

    /// Whether a shadow path of a key of this role is reported (never desired): STATE, and
    /// CONFIG (a device's configured identity, such as its SKU).
    ///
    /// @version 1
    /// @req REQ-ING-SHD-008
    pub fn reported(self) -> bool {
        matches!(self, Role::State | Role::Config)
    }
}

impl Kind {
    /// `DMT_PK_*` code.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-006
    pub fn code(self) -> u8 {
        match self {
            Kind::Unix => 0,
            Kind::Inproc => 1,
            Kind::Uart => 2,
        }
    }
}

impl PrincipalFlag {
    /// `DMT_PF_*` bit.
    ///
    /// @version 1
    /// @req REQ-ING-TIV-006
    pub fn bit(self) -> u8 {
        match self {
            PrincipalFlag::WatchdogRequired => 0x01,
            PrincipalFlag::TraceReader => 0x02,
            PrincipalFlag::TokenIssuer => 0x04,
        }
    }
}

/// What a well-known role requires of the key mapped to it (R-WK).
#[derive(Debug, Clone, Copy)]
pub struct WkRule {
    pub name: &'static str,
    pub required: bool,
    /// `None` for the command roles, which map a command key.
    pub ty: Option<KeyType>,
    pub persist: Persist,
    pub survives_reset: bool,
    /// Written only by the core: no principal may have write on it.
    pub core_written: bool,
    /// Smallest `max_size` the store core reads the key with: `min_size + per_principal * N`.
    pub min_size: u32,
    pub per_principal: u32,
}

/// Build one R-WK row.
///
/// v2: no minimum size unless a row sets one.
/// @version 2
/// @req REQ-ING-TIV-010
const fn wk(name: &'static str, required: bool, ty: Option<KeyType>, persist: Persist) -> WkRule {
    WkRule {
        name,
        required,
        ty,
        persist,
        survives_reset: false,
        core_written: ty.is_some(),
        min_size: 0,
        per_principal: 0,
    }
}

/// A core-written, non-persistent diagnostic counter.
///
/// @version 1
/// @req REQ-ING-TIV-010
const fn diag(name: &'static str, ty: KeyType) -> WkRule {
    wk(name, false, Some(ty), Persist::None)
}

/// R-WK in `DMT_WK_*` order; the first `WK_REQUIRED` are required.
pub const WK_RULES: [WkRule; 26] = [
    wk("RESET_REQUEST", true, None, Persist::None),
    wk("RESET_PRE_COMMAND", true, None, Persist::None),
    wk("TRACE_ENABLE", true, None, Persist::None),
    wk("SECRETS_FALLBACK", true, Some(KeyType::Bool), Persist::None),
    WkRule {
        survives_reset: true,
        ..wk("BOOT_EPOCH", true, Some(KeyType::Uint32), Persist::Durable)
    },
    wk(
        "DIAG_CORRUPTION",
        true,
        Some(KeyType::Uint32),
        Persist::Deferred,
    ),
    // "M.m.p+<12 hex commit>" is at most 30 bytes; "M.m.p" at most 17.
    WkRule {
        min_size: 30,
        ..wk(
            "DIAG_KEYSTORE_VERSION",
            true,
            Some(KeyType::String),
            Persist::None,
        )
    },
    WkRule {
        min_size: 17,
        ..wk(
            "DIAG_CONTRACT_VERSION",
            true,
            Some(KeyType::String),
            Persist::None,
        )
    },
    WkRule {
        survives_reset: true,
        core_written: false,
        ..wk("DEVICE_ID", true, Some(KeyType::String), Persist::Durable)
    },
    diag("DIAG_BYTES_WRITTEN_KIB", KeyType::Uint32),
    diag("DIAG_RECORDS_WRITTEN", KeyType::Uint32),
    diag("DIAG_FSYNC_COUNT", KeyType::Uint32),
    diag("DIAG_FSYNC_LATENCY_US", KeyType::Uint32),
    diag("DIAG_COMPACTIONS", KeyType::Uint32),
    diag("DIAG_JOURNAL_BYTES", KeyType::Uint32),
    diag("DIAG_FLASH_LIFE", KeyType::Uint8),
    diag("DIAG_OVERFLOW_RESYNCS", KeyType::Uint32),
    diag("DIAG_HANDSHAKE_REFUSALS", KeyType::Uint32),
    diag("DIAG_RIGHTS_REJECTIONS", KeyType::Uint32),
    diag("DIAG_RATE_REJECTIONS", KeyType::Uint32),
    diag("DIAG_REPLAY_SKIPPED", KeyType::Uint32),
    diag("DIAG_TORN_TAILS", KeyType::Uint32),
    diag("DIAG_ALLOC_GAPS", KeyType::Uint32),
    diag("DIAG_CONNECTED", KeyType::Uint32),
    // 4-byte header, then 20 bytes per principal.
    WkRule {
        min_size: 4,
        per_principal: 20,
        ..diag("DIAG_SUBSCRIBERS", KeyType::Binary)
    },
    diag("DIAG_TRACE_REFUSALS", KeyType::Uint32),
];

/// Index of the role named `name` in `WK_RULES`.
///
/// @version 1
/// @req REQ-ING-TIV-010
pub fn wk_index(name: &str) -> Option<usize> {
    WK_RULES.iter().position(|rule| rule.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parses_and_resolves_per_major() {
        let v = Version::parse("1.3").unwrap();
        assert_eq!(v, Version { major: 1, minor: 3 });
        assert_eq!(v.minor_in(1), 3);
        assert_eq!(v.minor_in(2), 0);
        assert_eq!(v.to_string(), "1.3");
        assert!(Version::parse("1").is_err());
        assert!(Version::parse("a.b").is_err());
        assert!(Version::parse("1.x").is_err());
    }

    /// Every key type in `DMT_T_*` order, with its scalar size and model name.
    const TYPES: [(KeyType, Option<u32>, &str); 11] = [
        (KeyType::Bool, Some(1), "bool"),
        (KeyType::Uint8, Some(1), "uint8"),
        (KeyType::Uint16, Some(2), "uint16"),
        (KeyType::Uint32, Some(4), "uint32"),
        (KeyType::Int8, Some(1), "int8"),
        (KeyType::Int16, Some(2), "int16"),
        (KeyType::Int32, Some(4), "int32"),
        (KeyType::Float32, Some(4), "float32"),
        (KeyType::String, None, "string"),
        (KeyType::Binary, None, "binary"),
        (KeyType::Struct, None, "struct"),
    ];

    #[test]
    fn codes_match_dmt_types() {
        for (code, (ty, size, name)) in TYPES.iter().enumerate() {
            assert_eq!(usize::from(ty.code()), code, "{name}");
            assert_eq!(ty.scalar_size(), *size, "{name}");
            assert_eq!(ty.name(), *name);
        }
        let persists = [Persist::None, Persist::Deferred, Persist::Durable];
        assert_eq!(persists.map(Persist::code), [0, 1, 2]);
        let roles = [
            Role::Settings,
            Role::State,
            Role::Command,
            Role::Config,
            Role::Diag,
        ];
        assert_eq!(roles.map(Role::code), [0, 1, 2, 3, 4]);
        assert_eq!(
            [Kind::Unix, Kind::Inproc, Kind::Uart].map(Kind::code),
            [0, 1, 2]
        );
        let flags = [
            PrincipalFlag::WatchdogRequired,
            PrincipalFlag::TraceReader,
            PrincipalFlag::TokenIssuer,
        ];
        assert_eq!(flags.map(PrincipalFlag::bit), [1, 2, 4]);
    }

    /// One frozen R-WK row: (name, type code or None, persistence code, survives reset, core
    /// written, min_size, per_principal).
    type RwkRow = (&'static str, Option<u8>, u8, bool, bool, u32, u32);

    /// R-WK as frozen, row for row; the first nine are required.
    const R_WK: [RwkRow; 26] = [
        ("RESET_REQUEST", None, 0, false, false, 0, 0),
        ("RESET_PRE_COMMAND", None, 0, false, false, 0, 0),
        ("TRACE_ENABLE", None, 0, false, false, 0, 0),
        ("SECRETS_FALLBACK", Some(0), 0, false, true, 0, 0),
        ("BOOT_EPOCH", Some(3), 2, true, true, 0, 0),
        ("DIAG_CORRUPTION", Some(3), 1, false, true, 0, 0),
        ("DIAG_KEYSTORE_VERSION", Some(8), 0, false, true, 30, 0),
        ("DIAG_CONTRACT_VERSION", Some(8), 0, false, true, 17, 0),
        ("DEVICE_ID", Some(8), 2, true, false, 0, 0),
        ("DIAG_BYTES_WRITTEN_KIB", Some(3), 0, false, true, 0, 0),
        ("DIAG_RECORDS_WRITTEN", Some(3), 0, false, true, 0, 0),
        ("DIAG_FSYNC_COUNT", Some(3), 0, false, true, 0, 0),
        ("DIAG_FSYNC_LATENCY_US", Some(3), 0, false, true, 0, 0),
        ("DIAG_COMPACTIONS", Some(3), 0, false, true, 0, 0),
        ("DIAG_JOURNAL_BYTES", Some(3), 0, false, true, 0, 0),
        ("DIAG_FLASH_LIFE", Some(1), 0, false, true, 0, 0),
        ("DIAG_OVERFLOW_RESYNCS", Some(3), 0, false, true, 0, 0),
        ("DIAG_HANDSHAKE_REFUSALS", Some(3), 0, false, true, 0, 0),
        ("DIAG_RIGHTS_REJECTIONS", Some(3), 0, false, true, 0, 0),
        ("DIAG_RATE_REJECTIONS", Some(3), 0, false, true, 0, 0),
        ("DIAG_REPLAY_SKIPPED", Some(3), 0, false, true, 0, 0),
        ("DIAG_TORN_TAILS", Some(3), 0, false, true, 0, 0),
        ("DIAG_ALLOC_GAPS", Some(3), 0, false, true, 0, 0),
        ("DIAG_CONNECTED", Some(3), 0, false, true, 0, 0),
        ("DIAG_SUBSCRIBERS", Some(9), 0, false, true, 4, 20),
        ("DIAG_TRACE_REFUSALS", Some(3), 0, false, true, 0, 0),
    ];

    #[test]
    fn wk_rules_are_r_wk_row_for_row() {
        for (i, (rule, row)) in WK_RULES.iter().zip(R_WK).enumerate() {
            let got = (
                rule.name,
                rule.ty.map(KeyType::code),
                rule.persist.code(),
                rule.survives_reset,
                rule.core_written,
                rule.min_size,
                rule.per_principal,
            );
            assert_eq!(got, row, "DMT_WK_ index {i}");
            assert_eq!(rule.required, i < 9, "{}", rule.name);
        }
    }

    #[test]
    fn wk_index_finds_roles_by_name() {
        assert_eq!(wk_index("RESET_REQUEST"), Some(0));
        assert_eq!(wk_index("DEVICE_ID"), Some(8));
        assert_eq!(wk_index("DIAG_TRACE_REFUSALS"), Some(25));
        assert_eq!(wk_index("NO_SUCH_ROLE"), None);
    }
}
