import { ref, reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface AppSettings {
  theme: string;
  auto_start: boolean;
  start_minimized: boolean;
}

export interface PiSettings {
  default_model: string;
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
  broker_ws_url: string;
  broker_http_url: string;
  uptime_secs: number;
  data_dir: string;
}

export interface PiAgentSettings {
  default_provider: string;
  default_model: string;
  default_thinking_level: string;
  packages: string[];
}

export function useAdmin() {
  const config = ref<AdminConfig | null>(null);
  const status = ref<AdminStatus | null>(null);
  const piSettings = ref<PiAgentSettings | null>(null);
  const loading = reactive({ config: false, status: false, piSettings: false });
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

  return {
    config,
    status,
    piSettings,
    loading,
    error,
    fetchConfig,
    saveConfig,
    fetchStatus,
    restartPi,
    stopPi,
    fetchPiAgentSettings,
  };
}
