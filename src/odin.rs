use std::fs;
use zed::lsp::{Completion, CompletionKind};
use zed::LanguageServerId;
use zed::{CodeLabel, CodeLabelSpan};
use zed_extension_api::{self as zed, settings::LspSettings, Result};

#[derive(Clone)]
struct OlsBinary {
    path: String,
    args: Option<Vec<String>>,
    environment: Option<Vec<(String, String)>>,
}

struct OdinExtension {
    cached_binary_path: Option<String>,
}

impl OdinExtension {
    fn language_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<OlsBinary> {
        let mut args: Option<Vec<String>> = None;

        // Get environment based on current platform
        let (platform, arch) = zed::current_platform();
        let environment = match platform {
            zed::Os::Mac | zed::Os::Linux => Some(worktree.shell_env()),
            zed::Os::Windows => None,
        };

        // LSP settings specified for ols
        if let Ok(lsp_settings) = LspSettings::for_worktree("ols", worktree) {
            if let Some(binary) = lsp_settings.binary {
                args = binary.arguments;
                if let Some(path) = binary.path {
                    return Ok(OlsBinary {
                        path: path.clone(),
                        args,
                        environment,
                    });
                }
            }
        }

        // Found ols in worktree, return it
        if let Some(path) = worktree.which("ols") {
            self.cached_binary_path = Some(path.clone());
            return Ok(OlsBinary {
                path,
                args,
                environment,
            });
        }

        // Binary location cached, return it
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map_or(false, |stat| stat.is_file()) {
                return Ok(OlsBinary {
                    path: path.clone(),
                    args,
                    environment,
                });
            }
        }

        // Update installation status to "Checking for Update"
        zed::set_language_server_installation_status(
            &language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        // Download the latest github release
        let release = zed::latest_github_release(
            "DanielGavin/ols",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: true,
            },
        )?;

        // Set the asset name's format based on the current arch and platform
        let asset_name = format!(
            "ols-{arch}-{os}.{extension}",
            arch = match arch {
                zed::Architecture::Aarch64 => "arm64",
                zed::Architecture::X86 => "x86",
                zed::Architecture::X8664 => "x86_64",
            },
            os = match platform {
                zed::Os::Mac => "darwin",
                zed::Os::Linux => "unknown-linux-gnu",
                zed::Os::Windows => "pc-windows-msvc",
            },
            extension = match platform {
                zed::Os::Mac | zed::Os::Linux => "zip",
                zed::Os::Windows => "zip",
            }
        );

        // Find the asset in the Github release, set the binary path and directory format
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("ols-{}", release.version);
        fs::create_dir_all(&version_dir)
            .map_err(|err| format!("failed to create directory '{version_dir}': {err}"))?;
        let binary_path = format!(
            "{version_dir}/ols-{arch}-{os}",
            arch = match arch {
                zed::Architecture::Aarch64 => "arm64",
                zed::Architecture::X86 => "x86",
                zed::Architecture::X8664 => "x86_64",
            },
            os = match platform {
                zed::Os::Mac => "darwin",
                zed::Os::Linux => "unknown-linux-gnu",
                zed::Os::Windows => "pc-windows-msvc",
            },
        );

        // If the language server binary is not found (not already downloaded), then download it, make it executable, and remove temp files.
        if !fs::metadata(&binary_path).map_or(false, |stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                &language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(
                &asset.download_url,
                &version_dir,
                match platform {
                    zed::Os::Mac | zed::Os::Linux => zed::DownloadedFileType::Zip,
                    zed::Os::Windows => zed::DownloadedFileType::Zip,
                },
            )
            .map_err(|e| format!("failed to download file: {e}"))?;

            zed::make_file_executable(&binary_path)?;

            let entries =
                fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    fs::remove_dir_all(&entry.path()).ok();
                }
            }
        }

        // Set the cached binary path and return it.
        self.cached_binary_path = Some(binary_path.clone());
        Ok(OlsBinary {
            path: binary_path,
            args,
            environment,
        })
    }
}

impl zed::Extension for OdinExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let ols_binary = self.language_server_binary(language_server_id, worktree)?;
        Ok(zed::Command {
            command: ols_binary.path,
            args: ols_binary.args.unwrap_or_default(),
            env: ols_binary.environment.unwrap_or_default(),
        })
    }
}

zed::register_extension!(OdinExtension);
