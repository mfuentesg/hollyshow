use std::collections::HashSet;

/// License family, version-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LicenseKind {
    Cc0,
    /// Public Domain Mark.
    Pdm,
    By,
    BySa,
    ByNc,
    ByNd,
    ByNcSa,
    ByNcNd,
    /// Anything we cannot classify (never allowed by default).
    Other,
}

/// A parsed license: family plus optional version (`4.0`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct License {
    pub kind: LicenseKind,
    pub version: Option<String>,
}

impl License {
    /// Parses Openverse slugs (`by-sa`, `cc0`, `pdm`) and Commons short names
    /// (`CC BY-SA 4.0`, `Public domain`, `PD-old`, `CC0`). `version` overrides a version found in `text`.
    pub fn parse(text: &str, version: Option<&str>) -> License {
        let t = text.trim().to_ascii_lowercase().replace(['_', '/'], " ");
        let t = t.replace("creative commons", "cc");
        let mut tokens: Vec<&str> = t.split_whitespace().collect();
        let mut ver = version
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_string());
        if let Some(last) = tokens.last() {
            if last.chars().next().is_some_and(|c| c.is_ascii_digit())
                && last.chars().all(|c| c.is_ascii_digit() || c == '.')
            {
                if ver.is_none() {
                    ver = Some((*last).to_string());
                }
                tokens.pop();
            }
        }
        let joined = tokens.join(" ");
        let slug = joined
            .trim_start_matches("cc ")
            .trim_start_matches("cc-")
            .to_string();
        let slug = slug.replace(' ', "-");
        let kind = match slug.as_str() {
            "0" | "cc0" | "zero" | "cc-0" | "cc0-1.0" | "cc-zero" => LicenseKind::Cc0,
            "pdm" | "public-domain" | "public-domain-mark" | "pd" => LicenseKind::Pdm,
            "by" | "attribution" => LicenseKind::By,
            "by-sa" | "sa-by" => LicenseKind::BySa,
            "by-nc" => LicenseKind::ByNc,
            "by-nd" => LicenseKind::ByNd,
            "by-nc-sa" => LicenseKind::ByNcSa,
            "by-nc-nd" => LicenseKind::ByNcNd,
            s if s.starts_with("pd-") || s.starts_with("public-domain") => LicenseKind::Pdm,
            _ => LicenseKind::Other,
        };
        let version = match kind {
            LicenseKind::Pdm | LicenseKind::Other => None,
            LicenseKind::Cc0 => ver.or_else(|| Some("1.0".into())),
            _ => ver,
        };
        License { kind, version }
    }

    /// Display form shown in badges and credits.
    pub fn display(&self) -> String {
        let base = match self.kind {
            LicenseKind::Cc0 => "CC0",
            LicenseKind::Pdm => "PDM",
            LicenseKind::By => "CC BY",
            LicenseKind::BySa => "CC BY-SA",
            LicenseKind::ByNc => "CC BY-NC",
            LicenseKind::ByNd => "CC BY-ND",
            LicenseKind::ByNcSa => "CC BY-NC-SA",
            LicenseKind::ByNcNd => "CC BY-NC-ND",
            LicenseKind::Other => return "Otra".to_string(),
        };
        match &self.version {
            Some(v) => format!("{base} {v}"),
            None => base.to_string(),
        }
    }

    /// Canonical deed URL, when it can be derived.
    pub fn url(&self) -> Option<String> {
        let v = self.version.as_deref();
        match self.kind {
            LicenseKind::Cc0 => Some(format!(
                "https://creativecommons.org/publicdomain/zero/{}/",
                v.unwrap_or("1.0")
            )),
            LicenseKind::Pdm => Some("https://creativecommons.org/publicdomain/mark/1.0/".into()),
            LicenseKind::Other => None,
            k => {
                let slug = match k {
                    LicenseKind::By => "by",
                    LicenseKind::BySa => "by-sa",
                    LicenseKind::ByNc => "by-nc",
                    LicenseKind::ByNd => "by-nd",
                    LicenseKind::ByNcSa => "by-nc-sa",
                    _ => "by-nc-nd",
                };
                Some(format!(
                    "https://creativecommons.org/licenses/{slug}/{}/",
                    v.unwrap_or("4.0")
                ))
            }
        }
    }
}

/// Which license families may be shown and downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicensePolicy {
    allowed: HashSet<LicenseKind>,
}

impl Default for LicensePolicy {
    /// Commercial use and modification allowed: CC0, PDM, CC BY, CC BY-SA.
    fn default() -> Self {
        Self {
            allowed: [
                LicenseKind::Cc0,
                LicenseKind::Pdm,
                LicenseKind::By,
                LicenseKind::BySa,
            ]
            .into_iter()
            .collect(),
        }
    }
}

impl LicensePolicy {
    /// Also lets NC/ND licenses through (an explicit user choice).
    pub fn permissive() -> Self {
        let mut p = Self::default();
        p.allowed.extend([
            LicenseKind::ByNc,
            LicenseKind::ByNd,
            LicenseKind::ByNcSa,
            LicenseKind::ByNcNd,
        ]);
        p
    }
    pub fn allows(&self, kind: LicenseKind) -> bool {
        self.allowed.contains(&kind)
    }
    /// True when the display string of a stored result is allowed.
    pub fn allows_text(&self, text: &str) -> bool {
        self.allows(License::parse(text, None).kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openverse_slugs() {
        let l = License::parse("by-sa", Some("4.0"));
        assert_eq!(l.kind, LicenseKind::BySa);
        assert_eq!(l.display(), "CC BY-SA 4.0");
        assert_eq!(License::parse("cc0", Some("1.0")).display(), "CC0 1.0");
        assert_eq!(License::parse("pdm", Some("1.0")).display(), "PDM");
        assert_eq!(
            License::parse("by-nc-nd", Some("2.0")).kind,
            LicenseKind::ByNcNd
        );
    }

    #[test]
    fn parses_commons_short_names() {
        assert_eq!(License::parse("CC BY-SA 4.0", None).kind, LicenseKind::BySa);
        assert_eq!(
            License::parse("CC BY 2.0", None).version.as_deref(),
            Some("2.0")
        );
        assert_eq!(License::parse("CC0", None).kind, LicenseKind::Cc0);
        assert_eq!(License::parse("Public domain", None).kind, LicenseKind::Pdm);
        assert_eq!(License::parse("PD-old-100", None).kind, LicenseKind::Pdm);
        assert_eq!(License::parse("CC BY-NC 3.0", None).kind, LicenseKind::ByNc);
        assert_eq!(License::parse("GFDL", None).kind, LicenseKind::Other);
        assert_eq!(
            License::parse("Copyrighted free use", None).kind,
            LicenseKind::Other
        );
        assert_eq!(License::parse("", None).kind, LicenseKind::Other);
    }

    #[test]
    fn default_policy_excludes_nc_nd_and_unknown() {
        let p = LicensePolicy::default();
        for ok in ["CC0 1.0", "PDM", "CC BY 4.0", "CC BY-SA 3.0"] {
            assert!(p.allows_text(ok), "{ok}");
        }
        for no in [
            "CC BY-NC 4.0",
            "CC BY-ND 2.0",
            "CC BY-NC-SA 4.0",
            "GFDL",
            "Otra",
        ] {
            assert!(!p.allows_text(no), "{no}");
        }
        assert!(LicensePolicy::permissive().allows(LicenseKind::ByNc));
        assert!(!LicensePolicy::permissive().allows(LicenseKind::Other));
    }

    #[test]
    fn builds_deed_urls() {
        assert_eq!(
            License::parse("by-sa", Some("4.0")).url().unwrap(),
            "https://creativecommons.org/licenses/by-sa/4.0/"
        );
        assert_eq!(
            License::parse("cc0", None).url().unwrap(),
            "https://creativecommons.org/publicdomain/zero/1.0/"
        );
    }
}
