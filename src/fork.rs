//! Fork builds: `herdr update` hands off to the fork's own update script.
//!
//! The official self-update downloads the upstream release over this binary,
//! which drops the fork's patches. A fork build instead runs
//! `scripts/fork-update.sh` from the checkout it was built from: that merges
//! the upstream release, tests, installs with live handoff, pushes, and
//! installs on the machines listed in `~/.config/herdr/fork-remotes`.
//! A copy installed on another machine has no checkout, so `herdr update`
//! there says where updates come from.
//!
//! `HERDR_FORK_UPSTREAM_UPDATE=1 herdr update` runs the upstream updater
//! anyway, for deliberately going back to the official release.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The checkout this binary was built from.
const CHECKOUT: &str = env!("CARGO_MANIFEST_DIR");
const UPDATE_SCRIPT: &str = "scripts/fork-update.sh";
const UPSTREAM_UPDATE_ENV: &str = "HERDR_FORK_UPSTREAM_UPDATE";

/// The commit `scripts/fork-install.sh` built, when it built this binary.
fn build_commit() -> Option<&'static str> {
    option_env!("HERDR_BUILD_COMMIT")
        .map(str::trim)
        .filter(|commit| !commit.is_empty())
}

fn label() -> String {
    match build_commit() {
        Some(commit) => format!("fork build {commit}"),
        None => "fork build".to_string(),
    }
}

fn update_script(checkout: &Path) -> Option<PathBuf> {
    let script = checkout.join(UPDATE_SCRIPT);
    script.is_file().then_some(script)
}

fn no_checkout_message(checkout: &str, label: &str) -> String {
    format!(
        "self-update is disabled for fork builds ({label}): `herdr update` would replace it with the upstream release. \
         Update from the machine that builds the fork by running `{checkout}/{UPDATE_SCRIPT}` there; \
         it installs on every host listed in its ~/.config/herdr/fork-remotes"
    )
}

/// Whether `herdr update` runs the fork's update instead of the upstream one.
pub(crate) fn replaces_self_update() -> bool {
    !upstream_update_requested(std::env::var(UPSTREAM_UPDATE_ENV).ok().as_deref())
}

fn upstream_update_requested(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.trim() == "1")
}

/// One line for `herdr channel set`, which would otherwise start an update.
pub(crate) fn update_guidance() -> &'static str {
    "Fork builds update with `herdr update`, which runs scripts/fork-update.sh from the fork checkout."
}

/// `herdr update` on a fork build: run the fork's update script, or explain
/// why this machine cannot. `None` means the upstream updater was requested.
pub(crate) fn self_update() -> Option<Result<(), String>> {
    replaces_self_update().then(run_update_script)
}

fn run_update_script() -> Result<(), String> {
    let Some(script) = update_script(Path::new(CHECKOUT)) else {
        return Err(no_checkout_message(CHECKOUT, &label()));
    };
    eprintln!("{}: running {}", label(), script.display());
    let status = Command::new("bash")
        .arg(&script)
        .current_dir(CHECKOUT)
        .status()
        .map_err(|err| format!("failed to run {}: {err}", script.display()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} exited with {status}", script.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_update_script_in_this_checkout() {
        assert_eq!(
            update_script(Path::new(CHECKOUT)),
            Some(Path::new(CHECKOUT).join(UPDATE_SCRIPT))
        );
    }

    #[test]
    fn a_missing_checkout_has_no_update_script() {
        assert_eq!(update_script(Path::new("/nonexistent/herdr")), None);
    }

    #[test]
    fn only_an_explicit_one_requests_the_upstream_updater() {
        assert!(upstream_update_requested(Some("1")));
        assert!(!upstream_update_requested(None));
        assert!(!upstream_update_requested(Some("")));
        assert!(!upstream_update_requested(Some("0")));
    }

    #[test]
    fn no_checkout_message_is_reported_as_disabled_self_update() {
        let message = no_checkout_message("/home/me/herdr", "fork build abc123");
        // main.rs prints this prefix without "update failed:"
        assert!(message.starts_with("self-update is disabled"));
        assert!(message.contains("/home/me/herdr/scripts/fork-update.sh"));
        assert!(message.contains("fork build abc123"));
    }
}
