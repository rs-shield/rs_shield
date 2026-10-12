//! Log filter for the CLI: `RUST_LOG` when set, WARN otherwise.

use tracing_subscriber::EnvFilter;

/// Level used when `RUST_LOG` is unset, empty or invalid.
pub const DEFAULT_LOG_FILTER: &str = "warn";

/// Third-party targets that would log credentials at their most verbose
/// levels: `aws_sigv4` traces the canonical request, whose signed headers
/// include `x-amz-security-token` (session credentials). They stay at WARN
/// unless `RUST_LOG` names them explicitly.
const SENSITIVE_TARGETS: &[&str] = &["aws_sigv4"];

/// Builds the tracing filter from the value of `RUST_LOG` (`None` when
/// unset). Returns the filter and, for an invalid value, a warning to show
/// once the subscriber is installed (the default is used instead).
pub fn log_filter(rust_log: Option<&str>) -> (EnvFilter, Option<String>) {
    let spec = rust_log.map(str::trim).filter(|s| !s.is_empty());
    let (mut filter, warning) = match spec {
        Some(spec) => match EnvFilter::builder().parse(spec) {
            Ok(f) => (f, None),
            Err(e) => (
                EnvFilter::new(DEFAULT_LOG_FILTER),
                Some(format!(
                    "Ignoring invalid RUST_LOG={:?} ({}); using '{}'",
                    spec, e, DEFAULT_LOG_FILTER
                )),
            ),
        },
        None => (EnvFilter::new(DEFAULT_LOG_FILTER), None),
    };
    for target in SENSITIVE_TARGETS {
        if !spec.is_some_and(|s| s.contains(target))
            && let Ok(d) = format!("{}=warn", target).parse()
        {
            filter = filter.add_directive(d);
        }
    }
    (filter, warning)
}

/// Installs the global subscriber (stderr, so command output on stdout
/// stays clean) with [`log_filter`] applied to `RUST_LOG`.
pub fn init_logging() {
    let rust_log = std::env::var("RUST_LOG").ok();
    let (filter, warning) = log_filter(rust_log.as_deref());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
    if let Some(w) = warning {
        tracing::warn!("{}", w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(rust_log: Option<&str>) -> String {
        log_filter(rust_log).0.to_string()
    }

    #[test]
    fn unset_or_empty_defaults_to_warn() {
        for v in [None, Some(""), Some("   ")] {
            let (f, w) = log_filter(v);
            assert!(w.is_none());
            assert_eq!(
                f.max_level_hint(),
                Some(tracing::level_filters::LevelFilter::WARN)
            );
        }
    }

    #[test]
    fn rust_log_is_honoured() {
        let (f, w) = log_filter(Some("debug"));
        assert!(w.is_none());
        assert_eq!(
            f.max_level_hint(),
            Some(tracing::level_filters::LevelFilter::DEBUG)
        );

        let s = shown(Some("rsb_sdk=trace,rsb_cli=info"));
        assert!(s.contains("rsb_sdk=trace"), "{}", s);
        assert!(s.contains("rsb_cli=info"), "{}", s);
    }

    #[test]
    fn invalid_rust_log_falls_back_to_warn_with_a_warning() {
        let (f, w) = log_filter(Some("rsb_sdk=notalevel"));
        assert!(w.unwrap().contains("RUST_LOG"));
        assert_eq!(
            f.max_level_hint(),
            Some(tracing::level_filters::LevelFilter::WARN)
        );
    }

    #[test]
    fn signing_traces_stay_quiet_unless_requested() {
        assert!(shown(Some("trace")).contains("aws_sigv4=warn"));
        assert!(shown(None).contains("aws_sigv4=warn"));
        let s = shown(Some("aws_sigv4=trace"));
        assert!(!s.contains("aws_sigv4=warn"), "{}", s);
    }
}
