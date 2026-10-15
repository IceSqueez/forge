use std::cmp::Ordering;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: (u64, u64, u64),
    pre: Vec<String>,
}

impl Version {
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (major, minor, patch) = self.core;
        write!(f, "{major}.{minor}.{patch}")?;
        if self.is_prerelease() {
            write!(f, "-{}", self.pre.join("."))?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| compare_prerelease(&self.pre, &other.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub fn parse_version(raw: &str) -> Option<Version> {
    let raw = raw.trim();
    let raw = raw.strip_prefix('v').unwrap_or(raw);
    let raw = raw.split('+').next().unwrap_or(raw);
    let (core, pre) = match raw.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (raw, None),
    };

    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }

    let pre = match pre {
        None => Vec::new(),
        Some(pre) => {
            let ids: Vec<String> = pre.split('.').map(str::to_owned).collect();
            if ids.iter().any(String::is_empty) {
                return None;
            }
            ids
        }
    };

    Some(Version {
        core: (major, minor, patch),
        pre,
    })
}

fn compare_prerelease(a: &[String], b: &[String]) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => {
            for (left, right) in a.iter().zip(b) {
                match compare_identifier(left, right) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
            a.len().cmp(&b.len())
        }
    }
}

fn compare_identifier(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

pub fn is_newer(current: &Version, latest: &Version) -> bool {
    if !current.is_prerelease() && latest.is_prerelease() {
        return false;
    }
    latest > current
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{Version, is_newer, parse_version};

    fn version(raw: &str) -> Version {
        parse_version(raw).unwrap()
    }

    #[test]
    fn parse_version_tolerates_a_v_prefix_build_metadata_and_surrounding_padding() {
        let plain = version("0.5.3");
        for raw in [
            " 0.5.3 ",
            "v0.5.3",
            "0.5.3+build.9",
            "v0.5.3+build.9",
            " v0.5.3 ",
        ] {
            assert_eq!(parse_version(raw).as_ref(), Some(&plain), "for {raw:?}");
        }
    }

    #[test]
    fn parse_version_keeps_a_prerelease_that_carries_build_metadata() {
        assert_eq!(
            parse_version("v0.5.3-beta.1+build.9"),
            parse_version("0.5.3-beta.1"),
        );
        assert_ne!(parse_version("0.5.3-beta.1"), parse_version("0.5.3"));
    }

    #[test]
    fn parse_version_rejects_anything_but_three_numeric_components() {
        for raw in [
            "",
            "   ",
            "0",
            "0.5",
            "0.5.3.1",
            "0.5.x",
            "0.-5.3",
            "next release",
            "not-a-version",
            "18446744073709551616.0.0",
        ] {
            assert!(parse_version(raw).is_none(), "accepted {raw:?}");
        }
    }

    #[test]
    fn parse_version_rejects_an_empty_prerelease_identifier() {
        for raw in ["0.5.3-", "0.5.3-beta.", "0.5.3-.1", "0.5.3-beta..1"] {
            assert!(parse_version(raw).is_none(), "accepted {raw:?}");
        }
    }

    #[test]
    fn version_display_round_trips_through_parse_without_build_metadata() {
        for (raw, shown) in [
            ("v0.6.2+130", "0.6.2"),
            ("0.6.2-beta.1+130", "0.6.2-beta.1"),
            (" 1.0.0-rc.2 ", "1.0.0-rc.2"),
        ] {
            let parsed = version(raw);
            assert_eq!(parsed.to_string(), shown, "for {raw:?}");
            assert_eq!(version(shown), parsed, "for {raw:?}");
        }
    }

    #[test]
    fn version_ordering_follows_semver_precedence() {
        for (lower, higher) in [
            ("0.5.2", "0.5.3"),
            ("0.5.9", "0.6.0"),
            ("0.9.9", "1.0.0"),
            ("0.5.3-beta.1", "0.5.3"),
            ("0.5.3-beta.2", "0.5.3-beta.10"),
            ("0.5.3-alpha.9", "0.5.3-beta.1"),
            ("1.0.0-alpha", "1.0.0-alpha.1"),
            ("1.0.0-1", "1.0.0-alpha"),
        ] {
            assert!(
                version(lower) < version(higher),
                "expected {lower} below {higher}",
            );
        }
    }

    #[test]
    fn is_newer_never_offers_a_prerelease_to_a_release_build() {
        for latest in ["0.5.3-beta.1", "0.6.0-rc.1", "1.0.0-alpha"] {
            assert!(
                !is_newer(&version("0.5.2"), &version(latest)),
                "offered {latest} to a release build",
            );
        }
    }

    #[test]
    fn is_newer_offers_a_release_build_only_a_strictly_higher_release() {
        for (latest, expected) in [
            ("0.5.3", true),
            ("1.0.0", true),
            ("0.5.2", false),
            ("v0.5.2+999", false),
            ("0.5.1", false),
        ] {
            assert_eq!(
                is_newer(&version("0.5.2"), &version(latest)),
                expected,
                "for {latest}",
            );
        }
    }

    #[test]
    fn is_newer_offers_a_prerelease_build_the_next_prerelease_and_the_release() {
        for (latest, expected) in [
            ("0.5.3-beta.2", true),
            ("0.5.3", true),
            ("0.5.4-beta.1", true),
            ("0.5.3-beta.1", false),
            ("0.5.3-alpha.9", false),
            ("0.5.2", false),
        ] {
            assert_eq!(
                is_newer(&version("0.5.3-beta.1"), &version(latest)),
                expected,
                "for {latest}",
            );
        }
    }
}
