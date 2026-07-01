pub mod extractor;
pub mod reconcile;

/// org_id=1 is the built-in system/Unassigned org; never a normal-user active org.
pub const SYSTEM_ORG_ID: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Owner,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Member => "member",
        }
    }

    // Defensive: an unknown/future role maps to least privilege, never an error.
    pub fn parse(s: &str) -> Role {
        match s {
            "owner" => Role::Owner,
            _ => Role::Member,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrgKind {
    Personal,
    Forseti,
    Native,
    System,
}

impl OrgKind {
    /// System wins over every other flag; then personal, then ext-linked (Forseti), else native.
    pub fn classify(org_id: i64, is_personal: bool, has_ext: bool) -> OrgKind {
        if org_id == SYSTEM_ORG_ID {
            OrgKind::System
        } else if is_personal {
            OrgKind::Personal
        } else if has_ext {
            OrgKind::Forseti
        } else {
            OrgKind::Native
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            OrgKind::Personal => "Personal",
            OrgKind::Forseti => "Forseti",
            OrgKind::Native => "Native",
            OrgKind::System => "System",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_parse_is_defensive() {
        assert_eq!(Role::parse("owner"), Role::Owner);
        assert_eq!(Role::parse("member"), Role::Member);
        assert_eq!(Role::parse("admin"), Role::Member); // unknown -> least privilege
        assert_eq!(Role::Owner.as_str(), "owner");
    }

    #[test]
    fn org_kind_classify_covers_every_variant() {
        assert_eq!(OrgKind::classify(SYSTEM_ORG_ID, false, false), OrgKind::System);
        // System wins even if other flags are set.
        assert_eq!(OrgKind::classify(SYSTEM_ORG_ID, true, true), OrgKind::System);
        assert_eq!(OrgKind::classify(5, true, false), OrgKind::Personal);
        assert_eq!(OrgKind::classify(5, false, true), OrgKind::Forseti);
        assert_eq!(OrgKind::classify(5, false, false), OrgKind::Native);
    }
}
