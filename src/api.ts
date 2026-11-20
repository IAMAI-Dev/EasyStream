import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  ConnectPeerRequest,
  ConnectPeerResponse,
  DecidePairingRequest,
  DecidePairingResult,
  FullSnapshot,
  GeneratedRoomCredentials,
  HostStatus,
  JoinNetworkRequest,
  PeersUpdatedEvent,
  RoomSnapshot,
  SessionSnapshot,
  TrustedClient,
} from "./types";

export const api = {
  getSnapshot: () => invoke<FullSnapshot>("get_snapshot"),
  generateRoomCredentials: () =>
    invoke<GeneratedRoomCredentials>("generate_room_credentials"),
  loadSavedRoom: () =>
    invoke<GeneratedRoomCredentials | null>("load_saved_room"),
  joinNetwork: (request: JoinNetworkRequest) =>
    invoke<RoomSnapshot>("join_network", { request }),
  leaveNetwork: () => invoke<void>("leave_network"),
  installSunshine: () => invoke<HostStatus>("install_sunshine"),
  toggleHostService: (enable: boolean) =>
    invoke<HostStatus>("toggle_host_service", { enable }),
  decidePairing: (request: DecidePairingRequest) =>
    invoke<DecidePairingResult>("decide_pairing", { request }),
  listTrustedClients: () => invoke<TrustedClient[]>("list_trusted_clients"),
  revokeClient: (clientId: string) =>
    invoke<boolean>("revoke_client", { clientId }),
  connectToPeer: (request: ConnectPeerRequest) =>
    invoke<ConnectPeerResponse>("connect_to_peer", { request }),
  disconnect: (operationId?: string | null) =>
    invoke<void>("disconnect", { operationId: operationId ?? null }),
  updateSettings: (settings: AppSettings) =>
    invoke<AppSettings>("update_settings", { settings }),
  exportDiagnostics: () => invoke<string>("export_diagnostics"),
  getLogs: () => invoke<string[]>("get_logs"),
};

export function subscribeEvents(handlers: {
  onRoomUpdated?: (room: RoomSnapshot) => void;
  onPeersUpdated?: (ev: PeersUpdatedEvent) => void;
  onHostUpdated?: (host: HostStatus) => void;
  onSessionUpdated?: (session: SessionSnapshot) => void;
  onSnapshotUpdated?: (snapshot: FullSnapshot) => void;
}): Promise<UnlistenFn[]> {
  const promises: Promise<UnlistenFn>[] = [];
  if (handlers.onRoomUpdated) {
    promises.push(
      listen<RoomSnapshot>("room-updated", (e) =>
        handlers.onRoomUpdated?.(e.payload)
      )
    );
  }
  if (handlers.onPeersUpdated) {
    promises.push(
      listen<PeersUpdatedEvent>("network-peers-updated", (e) =>
        handlers.onPeersUpdated?.(e.payload)
      )
    );
  }
  if (handlers.onHostUpdated) {
    promises.push(
      listen<HostStatus>("host-status-updated", (e) =>
        handlers.onHostUpdated?.(e.payload)
      )
    );
  }
  if (handlers.onSessionUpdated) {
    promises.push(
      listen<SessionSnapshot>("session-updated", (e) =>
        handlers.onSessionUpdated?.(e.payload)
      )
    );
  }
  if (handlers.onSnapshotUpdated) {
    promises.push(
      listen<FullSnapshot>("snapshot-updated", (e) =>
        handlers.onSnapshotUpdated?.(e.payload)
      )
    );
  }
  return Promise.all(promises);
}
