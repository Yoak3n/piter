import { ref, reactive } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";

export interface AppSettings {
  theme: string;
  /** "system" | "zh" | "en" — follows the OS locale when "system". */
  language: string;
  auto_start: boolean;
  start_minimized: boolean;
}

export interface PiSettings {
  request_timeout_secs: number;
  auto_restart_on_crash: boolean;
}

export interface AdminConfig {
  app: AppSettings;
  pi: PiSettings;
}

export interface SessionInfo {
  instance_id: string;
  session_path: string | null;
  cwd: string;
  state: string;
}

export interface AdminStatus {
  pi_running: boolean;
  active_sessions: SessionInfo[];
  pi_version: string;
  app_version: string;
  pi_binary_missing: boolean;
  broker_ws_url: string;
  broker_http_url: string;
  uptime_secs: number;
  data_dir: string;
}

export interface PiAgentSettings {
  defaultProvider: string;
  defaultModel: string;
  defaultThinkingLevel: string;
  // Pi 允许 packages 数组里每个元素是 source 字符串或过滤对象
  // （{ source, extensions, skills, ... }）。piter 不解释内容，原样透传。
  packages: Array<unknown>;
  skills?: string[];
}

export type PiOrigin = "downloaded" | "linked" | "missing";

export interface PiInstallInfo {
  version: string | null;
  origin: PiOrigin;
  binary_present: boolean;
  locked_version: string;
}

export interface ProjectBrief {
  id: string;
  name: string;
  cwd: string;
}

/** Progress events emitted by the `download_pi_version` command. */
export interface DownloadProgressEvent {
  stage: "downloading" | "extracting" | "verifying" | "done";
  downloaded?: number;
  total?: number;
  current?: number;
  total_entries?: number;
}

export interface ExtensionEntry {
  name: string;
  path: string | null;
}

export interface ProjectExtensionState extends ProjectBrief {
  extensions: ExtensionEntry[];
  /** Extensions this project adds on top of the global list. */
  added: string[];
  excluded: string[];
}

export interface ExtensionOverview {
  global_extensions: ExtensionEntry[];
  enabled_global: string[];
  projects: ProjectExtensionState[];
}

export type PiAuthSource = "stored" | "environment" | "none";

export interface PiProviderStatus {
  provider: string;
  display_name: string;
  configured: boolean;
  source: PiAuthSource;
  /** True for auth.json entries that are not known API-key providers (e.g. OAuth subscriptions). */
  custom: boolean;
  /** Environment variable providing the key (only when source is "environment"). */
  env_var: string | null;
}

// ─── Usage dashboard (get_cost_dashboard) ──────────────────────────────────

export interface CostDashboardRange {
  range: string;
  from: string;
  to: string;
}

export interface CostOverview {
  total_cost: number;
  sessions: number;
  messages: number;
  total_tokens: number;
  active_days: number;
  current_streak: number;
  longest_streak: number;
  input_tokens: number;
  output_tokens: number;
  cache_read: number;
  cache_write: number;
  tool_calls: number;
}

export interface CostToolStat {
  name: string;
  count: number;
  cost: number;
  fraction: number;
}

export interface CostModelStat {
  name: string;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cost: number;
  fraction: number;
}

export interface CostProjectStat {
  name: string;
  cwd: string;
  sessions: number;
  cost: number;
  fraction: number;
}

export interface CostSessionStat {
  title: string;
  workspace: string;
  model: string;
  total_tokens: number;
  tool_calls: number;
  total_cost: number;
  time: string;
}

export interface CostDailyPoint {
  key: string;
  total: number;
  models: Record<string, number>;
}

export interface CostDayActivity {
  key: string;
  value: number;
}

export interface CostDashboard {
  range: CostDashboardRange;
  overview: CostOverview;
  usage: {
    total_tokens: number;
    input_tokens: number;
    output_tokens: number;
    cache_read: number;
    cache_write: number;
    tool_calls: number;
    tools: CostToolStat[];
  };
  models: CostModelStat[];
  projects: CostProjectStat[];
  sessions: CostSessionStat[];
  daily: CostDailyPoint[];
  activity: CostDayActivity[];
}

// ─── App self-update (check_for_update / install_update) ──────────────────

export interface UpdateCheckInfo {
  current_version: string;
  latest_version: string;
  available: boolean;
  notes: string | null;
}

/** Progress events emitted by the `install_update` command. */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

export function useAdmin() {
  const config = ref<AdminConfig | null>(null);
  const status = ref<AdminStatus | null>(null);
  const piSettings = ref<PiAgentSettings | null>(null);
  const piInstall = ref<PiInstallInfo | null>(null);
  const downloadProgress = ref<DownloadProgressEvent | null>(null);
  const piAuthStatus = ref<PiProviderStatus[] | null>(null);
  const piModelsConfig = ref("");
  const loading = reactive({ config: false, status: false, piSettings: false, piInstall: false, downloading: false, uninstalling: false, authStatus: false, modelsConfig: false });
  const error = ref("");

  async function fetchConfig() {
    loading.config = true;
    error.value = "";
    try {
      config.value = await invoke<AdminConfig>("get_admin_config");
    } catch (e) {
      error.value = `Failed to load config: ${e}`;
    } finally {
      loading.config = false;
    }
  }

  async function saveConfig(cfg: AdminConfig): Promise<boolean> {
    loading.config = true;
    error.value = "";
    try {
      config.value = await invoke<AdminConfig>("update_admin_config", {
        config: cfg,
      });
      return true;
    } catch (e) {
      error.value = `Failed to save config: ${e}`;
      return false;
    } finally {
      loading.config = false;
    }
  }

  async function fetchStatus() {
    loading.status = true;
    error.value = "";
    try {
      status.value = await invoke<AdminStatus>("get_admin_status");
    } catch (e) {
      error.value = `Failed to load status: ${e}`;
    } finally {
      loading.status = false;
    }
  }

  async function restartPi(): Promise<string> {
    error.value = "";
    try {
      const msg = await invoke<string>("restart_pi");
      await fetchStatus();
      return msg;
    } catch (e) {
      error.value = `Failed to restart Pi: ${e}`;
      return String(e);
    }
  }

  async function stopPi(): Promise<string> {
    error.value = "";
    try {
      const msg = await invoke<string>("stop_pi");
      await fetchStatus();
      return msg;
    } catch (e) {
      error.value = `Failed to stop Pi: ${e}`;
      return String(e);
    }
  }

  async function fetchPiAgentSettings() {
    loading.piSettings = true;
    error.value = "";
    try {
      piSettings.value = await invoke<PiAgentSettings>("get_pi_agent_settings");
    } catch (e) {
      error.value = `Failed to read Pi settings: ${e}`;
    } finally {
      loading.piSettings = false;
    }
  }

  async function savePiAgentSettings(settings: PiAgentSettings): Promise<boolean> {
    loading.piSettings = true;
    error.value = "";
    try {
      await invoke("save_pi_agent_settings", { settings });
      piSettings.value = settings;
      return true;
    } catch (e) {
      error.value = `Failed to save Pi settings: ${e}`;
      return false;
    } finally {
      loading.piSettings = false;
    }
  }

  async function openPath(path: string) {
    try {
      await invoke("open_path", { path });
    } catch (e) {
      error.value = `Failed to open path: ${e}`;
    }
  }

  async function fetchPiInstallInfo() {
    loading.piInstall = true;
    error.value = "";
    try {
      piInstall.value = await invoke<PiInstallInfo>("get_pi_install_info");
    } catch (e) {
      error.value = `Failed to get Pi install info: ${e}`;
    } finally {
      loading.piInstall = false;
    }
  }

  async function downloadPiVersion(version: string): Promise<boolean> {
    loading.downloading = true;
    error.value = "";
    downloadProgress.value = null;
    try {
      // Stream progress from the Rust command via a channel — scoped to this
      // invoke call, no global event listeners needed.
      const channel = new Channel<DownloadProgressEvent>();
      channel.onmessage = (msg) => {
        downloadProgress.value = msg;
      };
      await invoke("download_pi_version", { version, onProgress: channel });
      // Pi is installed now — start the gateway so chat works without an app
      // restart.
      await startPiGateway();
      await fetchPiInstallInfo();
      await fetchStatus();
      return true;
    } catch (e) {
      error.value = `Failed to download Pi ${version}: ${e}`;
      return false;
    } finally {
      downloadProgress.value = null;
      loading.downloading = false;
    }
  }

  /** Start the gateway (after pi has been installed mid-session). */
  async function startPiGateway(): Promise<string | null> {
    try {
      return await invoke<string>("start_pi_gateway");
    } catch (e) {
      error.value = `Failed to start gateway: ${e}`;
      return null;
    }
  }

  async function uninstallPi(): Promise<boolean> {
    loading.uninstalling = true;
    error.value = "";
    try {
      await invoke("uninstall_pi");
      await fetchPiInstallInfo();
      await fetchStatus();
      return true;
    } catch (e) {
      error.value = `Failed to uninstall Pi: ${e}`;
      return false;
    } finally {
      loading.uninstalling = false;
    }
  }

  async function checkForUpdate(): Promise<UpdateCheckInfo | null> {
    error.value = "";
    try {
      return await invoke<UpdateCheckInfo>("check_for_update");
    } catch (e) {
      error.value = `Failed to check for update: ${e}`;
      return null;
    }
  }

  /** Download + install the pending update; the app relaunches on success. */
  async function installUpdate(
    onProgress: (p: UpdateProgress) => void,
    onError?: (msg: string) => void,
  ): Promise<boolean> {
    error.value = "";
    try {
      const channel = new Channel<UpdateProgress>();
      channel.onmessage = onProgress;
      await invoke("install_update", { onProgress: channel });
      return true;
    } catch (e) {
      error.value = `Failed to install update: ${e}`;
      onError?.(String(e));
      return false;
    }
  }

  async function fetchExtensionOverview(): Promise<ExtensionOverview | null> {
    error.value = "";
    try {
      return await invoke<ExtensionOverview>("get_extension_overview");
    } catch (e) {
      error.value = `Failed to load extension overview: ${e}`;
      return null;
    }
  }

  /** Lazily load one project's extension candidates (called when selected). */
  async function fetchProjectExtensionOverview(projectId: string): Promise<ProjectExtensionState | null> {
    error.value = "";
    try {
      return await invoke<ProjectExtensionState>("get_project_extension_overview", { projectId });
    } catch (e) {
      error.value = `Failed to load project extensions: ${e}`;
      return null;
    }
  }

  async function saveGlobalExtensions(extensions: string[]): Promise<boolean> {
    error.value = "";
    try {
      await invoke("set_global_extensions", { extensions });
      return true;
    } catch (e) {
      error.value = `Failed to save global extensions: ${e}`;
      return false;
    }
  }

  async function saveProjectAddedExtensions(projectId: string, extensions: string[]): Promise<boolean> {
    error.value = "";
    try {
      await invoke("set_project_added_extensions", { projectId, extensions });
      return true;
    } catch (e) {
      error.value = `Failed to save project extensions: ${e}`;
      return false;
    }
  }

  async function saveProjectExcludedExtensions(projectId: string, extensions: string[]): Promise<boolean> {
    error.value = "";
    try {
      await invoke("set_project_excluded_extensions", { projectId, extensions });
      return true;
    } catch (e) {
      error.value = `Failed to save project exclusions: ${e}`;
      return false;
    }
  }

  async function fetchPiAuthStatus() {
    loading.authStatus = true;
    error.value = "";
    try {
      piAuthStatus.value = await invoke<PiProviderStatus[]>("list_pi_auth_status");
    } catch (e) {
      error.value = `Failed to read Pi credentials: ${e}`;
    } finally {
      loading.authStatus = false;
    }
  }

  async function setPiApiKey(provider: string, apiKey: string): Promise<boolean> {
    error.value = "";
    try {
      await invoke("set_pi_api_key", { provider, apiKey });
      await fetchPiAuthStatus();
      return true;
    } catch (e) {
      error.value = `Failed to save API key: ${e}`;
      return false;
    }
  }

  async function removePiApiKey(provider: string): Promise<boolean> {
    error.value = "";
    try {
      await invoke("remove_pi_api_key", { provider });
      await fetchPiAuthStatus();
      return true;
    } catch (e) {
      error.value = `Failed to remove API key: ${e}`;
      return false;
    }
  }

  async function fetchPiModelsConfig() {
    loading.modelsConfig = true;
    error.value = "";
    try {
      piModelsConfig.value = await invoke<string>("get_pi_models_config");
    } catch (e) {
      error.value = `Failed to read models config: ${e}`;
    } finally {
      loading.modelsConfig = false;
    }
  }

  async function savePiModelsConfig(content: string): Promise<boolean> {
    loading.modelsConfig = true;
    error.value = "";
    try {
      await invoke("save_pi_models_config", { content });
      piModelsConfig.value = content;
      return true;
    } catch (e) {
      error.value = `Failed to save models config: ${e}`;
      return false;
    } finally {
      loading.modelsConfig = false;
    }
  }

  return {
    config,
    status,
    piSettings,
    piInstall,
    downloadProgress,
    piAuthStatus,
    piModelsConfig,
    loading,
    error,
    fetchConfig,
    saveConfig,
    fetchStatus,
    restartPi,
    stopPi,
    fetchPiAgentSettings,
    savePiAgentSettings,
    openPath,
    fetchPiInstallInfo,
    downloadPiVersion,
    startPiGateway,
    uninstallPi,
    fetchExtensionOverview,
    fetchProjectExtensionOverview,
    saveGlobalExtensions,
    saveProjectAddedExtensions,
    saveProjectExcludedExtensions,
    fetchPiAuthStatus,
    setPiApiKey,
    removePiApiKey,
    fetchPiModelsConfig,
    savePiModelsConfig,
    checkForUpdate,
    installUpdate,
  };
}
