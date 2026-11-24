//! The registry of ACP agents Anode can launch.
//!
//! A provider is one configured local agent process: a display name, the
//! executable and arguments that start it in ACP mode, and the environment it
//! runs with. Codex and OpenCode ship as built-ins; any other stdio ACP agent
//! can be added as a custom provider, which is also how an OpenAI- or
//! Anthropic-compatible base URL and API key reach the agent.
//!
//! Installation and sign-in state always come from the provider's own commands.
//! Finding an executable proves it is installed, never that it is signed in.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Stable identifier for a configured provider.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub(crate) struct ProviderId(String);

impl ProviderId {
    pub(crate) const CODEX: &'static str = "codex";
    pub(crate) const OPENCODE: &'static str = "opencode";

    pub(crate) fn codex() -> Self {
        Self(Self::CODEX.into())
    }

    pub(crate) fn opencode() -> Self {
        Self(Self::OPENCODE.into())
    }

    pub(crate) fn custom() -> Self {
        Self(format!("custom-{}", uuid::Uuid::new_v4()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn is_builtin(&self) -> bool {
        matches!(self.0.as_str(), Self::CODEX | Self::OPENCODE)
    }
}

impl From<String> for ProviderId {
    fn from(value: String) -> Self {
        // Transcripts saved before providers had ids stored the enum variant
        // name, so an existing chat keeps working after the upgrade.
        match value.as_str() {
            "Codex" => Self::codex(),
            "OpenCode" => Self::opencode(),
            _ => Self(value),
        }
    }
}

impl From<ProviderId> for String {
    fn from(value: ProviderId) -> Self {
        value.0
    }
}

impl std::fmt::Display for ProviderId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What kind of agent a provider runs, which decides its defaults, install
/// packages, and sign-in command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderKind {
    #[default]
    Codex,
    OpenCode,
    Custom,
}

/// One npm package a built-in provider needs, and the executable it provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InstallStep {
    pub(crate) package: &'static str,
    pub(crate) binary: &'static str,
    pub(crate) purpose: &'static str,
}

impl InstallStep {
    pub(crate) fn is_installed(&self) -> bool {
        crate::toolchain::locate_executable(self.binary).is_some()
    }
}

impl ProviderKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
            Self::Custom => "Custom agent",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            Self::Codex => "OpenAI Codex through the codex-acp stdio adapter, using your ChatGPT or API account.",
            Self::OpenCode => "The OpenCode CLI in ACP mode, with the models your OpenCode account already has.",
            Self::Custom => "Any stdio agent that speaks ACP v1, started with your own command and environment.",
        }
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Codex => "codex-acp",
            Self::OpenCode => "opencode",
            Self::Custom => "",
        }
    }

    pub(crate) fn args(self) -> Vec<String> {
        match self {
            Self::OpenCode => vec!["acp".into()],
            _ => Vec::new(),
        }
    }

    /// Packages Anode can install for this kind, in the order they are needed.
    pub(crate) fn install_steps(self) -> &'static [InstallStep] {
        match self {
            Self::Codex => &[
                InstallStep {
                    package: "@openai/codex",
                    binary: "codex",
                    purpose: "Codex CLI and its ChatGPT sign-in",
                },
                InstallStep {
                    package: "@agentclientprotocol/codex-acp",
                    binary: "codex-acp",
                    purpose: "ACP adapter Anode talks to",
                },
            ],
            Self::OpenCode => &[InstallStep {
                package: "opencode-ai",
                binary: "opencode",
                purpose: "OpenCode CLI, which serves ACP",
            }],
            Self::Custom => &[],
        }
    }

    /// The executable that owns sign-in for this kind, when it has one.
    pub(crate) fn auth_binary(self) -> Option<&'static str> {
        match self {
            Self::Codex => Some("codex"),
            Self::OpenCode => Some("opencode"),
            Self::Custom => None,
        }
    }

    pub(crate) fn guidance(self) -> &'static str {
        match self {
            Self::Codex => {
                "Codex signs in through your ChatGPT account. Anode never stores the credential; the Codex CLI owns it."
            }
            Self::OpenCode => {
                "OpenCode keeps its own credentials. `opencode auth login` is interactive, so Anode opens it in the Terminal."
            }
            Self::Custom => {
                "Anode starts this command with your environment and speaks ACP v1 over its stdin and stdout."
            }
        }
    }
}

/// One configured provider.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProviderProfile {
    pub(crate) id: ProviderId,
    #[serde(default)]
    pub(crate) kind: ProviderKind,
    pub(crate) name: String,
    pub(crate) command: String,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    /// Extra environment for the agent process. A base URL and API key for an
    /// OpenAI- or Anthropic-compatible endpoint belong here.
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub(crate) enabled: bool,
}

fn default_true() -> bool {
    true
}

impl ProviderProfile {
    pub(crate) fn builtin(kind: ProviderKind) -> Self {
        let id = match kind {
            ProviderKind::Codex => ProviderId::codex(),
            ProviderKind::OpenCode => ProviderId::opencode(),
            ProviderKind::Custom => ProviderId::custom(),
        };
        Self {
            id,
            kind,
            name: kind.label().into(),
            command: kind.command().into(),
            args: kind.args(),
            env: BTreeMap::new(),
            enabled: true,
        }
    }

    pub(crate) fn custom(name: impl Into<String>) -> Self {
        Self {
            id: ProviderId::custom(),
            kind: ProviderKind::Custom,
            name: name.into(),
            command: String::new(),
            args: Vec::new(),
            env: BTreeMap::new(),
            enabled: true,
        }
    }

    /// True when nothing about the launch command has been customized.
    pub(crate) fn is_default(&self) -> bool {
        self.kind != ProviderKind::Custom
            && self.command == self.kind.command()
            && self.args == self.kind.args()
            && self.env.is_empty()
    }

    pub(crate) fn resolve(&self) -> Result<PathBuf, String> {
        let command = self.command.trim();
        if command.is_empty() {
            return Err(format!("{} has no command set.", self.name));
        }
        let path = if std::path::Path::new(command).is_absolute() {
            Some(PathBuf::from(command))
        } else {
            crate::toolchain::locate_executable(command)
        };
        path.and_then(|path| path.canonicalize().ok())
            .filter(|path| path.is_file())
            .ok_or_else(|| format!("Cannot find `{command}`."))
    }
}

/// Every configured provider, in the order the settings screen lists them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Registry {
    #[serde(default)]
    providers: Vec<ProviderProfile>,
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            providers: vec![
                ProviderProfile::builtin(ProviderKind::Codex),
                ProviderProfile::builtin(ProviderKind::OpenCode),
            ],
        }
    }
}

impl Registry {
    pub(crate) fn load() -> Self {
        let mut registry = std::fs::read(registry_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .unwrap_or_else(Self::migrated);
        // A built-in is never missing, even from a hand-edited file.
        for kind in [ProviderKind::Codex, ProviderKind::OpenCode] {
            let profile = ProviderProfile::builtin(kind);
            if !registry.providers.iter().any(|p| p.id == profile.id) {
                registry.providers.insert(0, profile);
            }
        }
        registry
    }

    /// Pick up the one-file-per-provider layout Anode wrote before custom
    /// providers existed.
    fn migrated() -> Self {
        let mut registry = Self::default();
        for (id, legacy) in [
            (ProviderId::codex(), "codex.json"),
            (ProviderId::opencode(), "opencode.json"),
        ] {
            let Ok(bytes) = std::fs::read(agents_dir().join(legacy)) else {
                continue;
            };
            let Ok(legacy) = serde_json::from_slice::<LegacyProfile>(&bytes) else {
                continue;
            };
            if let Some(profile) = registry.providers.iter_mut().find(|p| p.id == id) {
                profile.command = legacy.executable;
                profile.args = legacy.args;
            }
        }
        registry
    }

    pub(crate) fn save(&self) -> Result<(), String> {
        let path = registry_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        // The file can hold API keys for custom agents.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }

    pub(crate) fn all(&self) -> &[ProviderProfile] {
        &self.providers
    }

    pub(crate) fn get(&self, id: &ProviderId) -> Option<&ProviderProfile> {
        self.providers.iter().find(|profile| profile.id == *id)
    }

    pub(crate) fn contains(&self, id: &ProviderId) -> bool {
        self.get(id).is_some()
    }

    /// The first enabled provider, used only to decide what the settings
    /// screen opens on. Chat never selects a provider on its own.
    pub(crate) fn first_id(&self) -> Option<ProviderId> {
        self.providers.first().map(|profile| profile.id.clone())
    }

    pub(crate) fn upsert(&mut self, profile: ProviderProfile) {
        match self
            .providers
            .iter_mut()
            .find(|existing| existing.id == profile.id)
        {
            Some(existing) => *existing = profile,
            None => self.providers.push(profile),
        }
    }

    /// Remove a custom provider. Built-ins reset to their defaults instead.
    pub(crate) fn remove(&mut self, id: &ProviderId) -> bool {
        if id.is_builtin() {
            if let Some(profile) = self.providers.iter_mut().find(|p| p.id == *id) {
                let kind = profile.kind;
                *profile = ProviderProfile::builtin(kind);
                return true;
            }
            return false;
        }
        let before = self.providers.len();
        self.providers.retain(|profile| profile.id != *id);
        before != self.providers.len()
    }
}

#[derive(Deserialize)]
struct LegacyProfile {
    executable: String,
    #[serde(default)]
    args: Vec<String>,
}

/// What a provider's own commands report about its sign-in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum AuthState {
    /// Not probed yet, or the provider owns no sign-in Anode can read.
    #[default]
    Unknown,
    /// The agent runs on whatever credentials its environment supplies.
    NotApplicable,
    SignedOut,
    SignedIn(String),
}

impl AuthState {
    pub(crate) fn is_signed_in(&self) -> bool {
        matches!(self, Self::SignedIn(_))
    }
}

/// Everything a probe learned about one provider.
#[derive(Clone, Debug)]
pub(crate) struct ProviderStatus {
    pub(crate) resolved_path: Option<PathBuf>,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) auth: AuthState,
    pub(crate) auth_detail: Option<String>,
    /// Packages this provider still needs, in install order.
    pub(crate) missing: Vec<InstallStep>,
}

impl ProviderStatus {
    pub(crate) fn is_available(&self) -> bool {
        self.resolved_path.is_some()
    }
}

/// Resolve the executable, read its version, and ask the provider itself about
/// sign-in. Blocking; callers run it off the UI thread.
pub(crate) fn probe_provider(profile: &ProviderProfile) -> ProviderStatus {
    let missing: Vec<InstallStep> = profile
        .kind
        .install_steps()
        .iter()
        .copied()
        .filter(|step| !step.is_installed())
        .collect();
    let (resolved_path, version, error) = match profile.resolve() {
        Ok(executable) => {
            let version = read_line(&executable, &["--version"], profile);
            (Some(executable), version, None)
        }
        Err(error) => (None, None, Some(error)),
    };
    let (auth, auth_detail) = probe_auth(profile);
    ProviderStatus {
        resolved_path,
        version,
        error,
        auth,
        auth_detail,
        missing,
    }
}

fn probe_auth(profile: &ProviderProfile) -> (AuthState, Option<String>) {
    let Some(binary) = profile.kind.auth_binary() else {
        return (AuthState::NotApplicable, None);
    };
    let Some(executable) = crate::toolchain::locate_executable(binary) else {
        return (AuthState::Unknown, None);
    };
    match profile.kind {
        ProviderKind::Codex => {
            let output = run(&executable, &["login", "status"], profile);
            let Some(output) = output else {
                return (AuthState::Unknown, None);
            };
            let text = output.text.trim().to_string();
            if output.success && !text.to_lowercase().contains("not logged in") {
                (AuthState::SignedIn(codex_account(&text)), Some(text))
            } else {
                (AuthState::SignedOut, Some(text))
            }
        }
        ProviderKind::OpenCode => {
            let output = run(&executable, &["auth", "list"], profile);
            let Some(output) = output else {
                return (AuthState::Unknown, None);
            };
            let accounts = opencode_accounts(&output.text);
            if accounts.is_empty() {
                (AuthState::SignedOut, None)
            } else {
                let mut shown: Vec<String> = accounts.iter().take(3).cloned().collect();
                if accounts.len() > shown.len() {
                    shown.push(format!("and {} more", accounts.len() - shown.len()));
                }
                (
                    AuthState::SignedIn(format!(
                        "{} credential{}",
                        accounts.len(),
                        if accounts.len() == 1 { "" } else { "s" }
                    )),
                    Some(shown.join(", ")),
                )
            }
        }
        ProviderKind::Custom => (AuthState::NotApplicable, None),
    }
}

/// How `codex login status` names the signed-in account.
///
/// Codex 0.167 prints `Logged in using ChatGPT` on stderr, so the account is
/// what follows that phrase; an unrecognized line is shown as it came.
fn codex_account(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Signed in")
        .trim();
    let account = line
        .strip_prefix("Logged in using ")
        .or_else(|| line.strip_prefix("Logged in as "))
        .unwrap_or(line)
        .trim();
    if account.is_empty() {
        "Signed in".into()
    } else {
        account.into()
    }
}

/// The provider names in `opencode auth list` output, which prints one
/// credential per line as `<provider>  <method>  <source>`.
fn opencode_accounts(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| line.split("  ").find(|part| !part.trim().is_empty()))
        .map(|name| name.trim().to_string())
        .collect()
}

pub(crate) struct CommandOutput {
    pub(crate) success: bool,
    pub(crate) text: String,
}

/// Run a short, read-only provider command and collect its combined output.
pub(crate) fn run(
    executable: &std::path::Path,
    args: &[&str],
    profile: &ProviderProfile,
) -> Option<CommandOutput> {
    let mut command = Command::new(executable);
    if let Some(path) = crate::toolchain::subprocess_path(executable) {
        command.env("PATH", path);
    }
    for (name, value) in &profile.env {
        command.env(name, value);
    }
    let output = command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .ok()?;
    let mut text = crate::device::strip_ansi(&String::from_utf8_lossy(&output.stdout));
    let errors = crate::device::strip_ansi(&String::from_utf8_lossy(&output.stderr));
    if text.trim().is_empty() {
        text = errors;
    }
    Some(CommandOutput {
        success: output.status.success(),
        text: super::transport::redact(&text),
    })
}

fn read_line(
    executable: &std::path::Path,
    args: &[&str],
    profile: &ProviderProfile,
) -> Option<String> {
    let output = run(executable, args, profile)?;
    output
        .text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
}

fn agents_dir() -> PathBuf {
    std::env::var_os("ANODE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::config_dir().map(|dir| dir.join("anode")))
        .unwrap_or_else(std::env::temp_dir)
        .join("agents")
}

fn registry_path() -> PathBuf {
    agents_dir().join("providers.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_provider_names_in_saved_chats_resolve_to_current_ids() {
        let id: ProviderId = serde_json::from_str(r#""Codex""#).unwrap();
        assert_eq!(id, ProviderId::codex());
        let id: ProviderId = serde_json::from_str(r#""OpenCode""#).unwrap();
        assert_eq!(id, ProviderId::opencode());
        let id: ProviderId = serde_json::from_str(r#""custom-7""#).unwrap();
        assert_eq!(id.as_str(), "custom-7");
        assert_eq!(serde_json::to_string(&id).unwrap(), r#""custom-7""#);
    }

    #[test]
    fn registry_keeps_builtins_and_only_deletes_custom_providers() {
        let mut registry = Registry::default();
        assert_eq!(registry.all().len(), 2);

        let custom = ProviderProfile::custom("My agent");
        let custom_id = custom.id.clone();
        registry.upsert(custom);
        assert_eq!(registry.all().len(), 3);
        assert!(registry.remove(&custom_id));
        assert!(!registry.contains(&custom_id));

        let mut codex = registry.get(&ProviderId::codex()).unwrap().clone();
        assert!(codex.is_default());
        codex.command = "/opt/codex-acp".into();
        registry.upsert(codex);
        assert!(!registry.get(&ProviderId::codex()).unwrap().is_default());
        // Removing a built-in restores its defaults rather than dropping it.
        assert!(registry.remove(&ProviderId::codex()));
        assert!(registry.get(&ProviderId::codex()).unwrap().is_default());
        assert_eq!(registry.all().len(), 2);
    }

    #[test]
    fn the_codex_account_comes_from_its_own_login_status() {
        // Verbatim output of codex-cli 0.167, which prints it on stderr.
        assert_eq!(codex_account("Logged in using ChatGPT"), "ChatGPT");
        assert_eq!(
            codex_account("Logged in as ada@example.com"),
            "ada@example.com"
        );
        // An unrecognized line is reported as it came rather than guessed at.
        assert_eq!(
            codex_account("Authenticated (api key)"),
            "Authenticated (api key)"
        );
        assert_eq!(codex_account(""), "Signed in");
    }

    #[test]
    fn opencode_credentials_are_read_from_its_own_auth_list() {
        // Verbatim shape of `opencode auth list` on a signed-in machine.
        let text = "OpenCode Go        API key                     stored\n\
                    Amazon Bedrock     API key                     stored\n\
                    OpenRouter         API key                     stored\n\
                    OpenRouter         OPENROUTER_API_KEY          environment\n";
        assert_eq!(
            opencode_accounts(text),
            ["OpenCode Go", "Amazon Bedrock", "OpenRouter", "OpenRouter"]
        );
        assert!(opencode_accounts("").is_empty());
        assert!(opencode_accounts("   \n\n").is_empty());
    }

    #[test]
    fn a_provider_without_a_command_reports_what_is_missing() {
        let profile = ProviderProfile::custom("Nameless");
        assert!(profile.resolve().unwrap_err().contains("no command"));
        let status = probe_provider(&profile);
        assert!(!status.is_available());
        assert_eq!(status.auth, AuthState::NotApplicable);
    }
}
