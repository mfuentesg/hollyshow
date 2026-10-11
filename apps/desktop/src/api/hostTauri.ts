import { invoke } from "@tauri-apps/api/core";
import { HOST_COMMANDS, type HostApi } from "./host";

/** `HostApi` over Tauri IPC. Argument names are camelCase (Tauri converts them to snake_case). */
export function createTauriHost(): HostApi {
  const call =
    <T>(name: keyof typeof HOST_COMMANDS) =>
    (args?: Record<string, unknown>) =>
      invoke<T>(HOST_COMMANDS[name], args);

  return {
    getSetting: (key) => call<string | null>("getSetting")({ key }),
    setSetting: (key, value) => call<void>("setSetting")({ key, value }),
    getActivePlaylistId: () => call<string | null>("getActivePlaylistId")(),
    saveSong: (song) => call<typeof song>("saveSong")({ song }),
    deleteSong: (id) => call<void>("deleteSong")({ id }),
    importSongs: (files) =>
      call<Awaited<ReturnType<HostApi["importSongs"]>>>("importSongs")({ files }),
    savePlaylist: (playlist) => call<typeof playlist>("savePlaylist")({ playlist }),
    deletePlaylist: (id) => call<void>("deletePlaylist")({ id }),
    saveTheme: (theme) => call<typeof theme>("saveTheme")({ theme }),
    deleteTheme: (id) => call<void>("deleteTheme")({ id }),
    listBibleCatalog: () =>
      call<Awaited<ReturnType<HostApi["listBibleCatalog"]>>>("listBibleCatalog")(),
    installBible: (code) =>
      call<Awaited<ReturnType<HostApi["installBible"]>>>("installBible")({ code }),
    importBible: (req) => call<Awaited<ReturnType<HostApi["importBible"]>>>("importBible")({ req }),
    removeBible: (code) => call<void>("removeBible")({ code }),
    updateBibleVersion: (code, patch) =>
      call<Awaited<ReturnType<HostApi["updateBibleVersion"]>>>("updateBibleVersion")({
        code,
        patch,
      }),
    reorderBibleVersions: (codes) =>
      call<Awaited<ReturnType<HostApi["reorderBibleVersions"]>>>("reorderBibleVersions")({ codes }),
    listMonitors: () => call<Awaited<ReturnType<HostApi["listMonitors"]>>>("listMonitors")(),
    openOutput: (id, monitor) =>
      id === "stage"
        ? invoke<void>("open_stage", { monitor })
        : call<void>("openOutput")({ id, monitor }),
    closeOutput: (id) =>
      call<void>("closeOutput")({ label: id === "stage" ? "stage" : `output-${id}` }),
    outputMonitors: () => call<Awaited<ReturnType<HostApi["outputMonitors"]>>>("outputMonitors")(),
    getPairing: () => call<Awaited<ReturnType<HostApi["getPairing"]>>>("getPairing")(),
    regeneratePin: () => call<Awaited<ReturnType<HostApi["regeneratePin"]>>>("regeneratePin")(),
    setPairingRole: (role) =>
      call<Awaited<ReturnType<HostApi["setPairingRole"]>>>("setPairingRole")({ role }),
    setRemoteEnabled: (enabled) =>
      call<Awaited<ReturnType<HostApi["setRemoteEnabled"]>>>("setRemoteEnabled")({ enabled }),
    listDevices: () => call<Awaited<ReturnType<HostApi["listDevices"]>>>("listDevices")(),
    kickDevice: (id) => call<void>("kickDevice")({ id }),
    health: () => call<Awaited<ReturnType<HostApi["health"]>>>("health")(),
    startupInfo: () => call<Awaited<ReturnType<HostApi["startupInfo"]>>>("startupInfo")(),
  };
}
