export type ErrorCode =
  | "NETWORK_JOIN_FAILED"
  | "NO_REACHABLE_PATH"
  | "UNAUTHORIZED"
  | "HOST_BUSY"
  | "APP_MISSING"
  | "ENCODE_OR_CAPTURE_FAILED"
  | "PAIRING_TIMEOUT"
  | "PAIRING_REJECTED"
  | "PAIRING_INVALID_PIN"
  | "HOST_IDENTITY_MISMATCH"
  | "COMPONENT_MISSING"
  | "SUNSHINE_CONFLICT"
  | "PERMISSION_REQUIRED"
  | "SUBNET_OVERLAP"
  | "OPERATION_CANCELLED"
  | "INVALID_ARGUMENT"
  | "INTERNAL_ERROR";

export interface CommandError {
  code: ErrorCode;
  message: string;
  detail?: string | null;
  recoverable: boolean;
}

export type AddressMode =
  | "dhcp"
  | { static: { ipv4Cidr: string } };

export type RoomStage =
  | "disconnected"
  | "joining"
  | "inRoom"
  | "reconnecting"
  | "error";

export interface SubnetWarning {
  localInterface: string;
  localCidr: string;
  virtualCidr: string;
  suggestion: string;
}

export interface RoomSnapshot {
  roomId: string | null;
  roomName: string | null;
  stage: RoomStage;
  localVirtualIp: string | null;
  localHostname: string;
  natType: string;
  rpcPort: number;
  relayNodes: string[];
  addressMode: AddressMode;
  subnetWarnings: SubnetWarning[];
  lastError: CommandError | null;
  hasSavedSecret: boolean;
}

export type RoutePath = "direct" | "relay" | "unknown";

export type HostReadiness =
  | "inRoom"
  | "needsPairing"
  | "streamReady"
  | "hostBusy"
  | "identityChanged"
  | "hostUnreachable";

export interface PeerInfo {
  peerId: string;
  hostname: string;
  virtualIp: string;
  rttMs: number | null;
  lossRate: number | null;
  path: RoutePath;
  tunnelProto: string;
  natType: string;
  readiness: HostReadiness;
  hostReachable: boolean;
  streamAvailable: boolean;
  paired: boolean;
  hostUniqueId: string | null;
  availableApps: string[];
  selectedApp: string | null;
  lastSeenMs: number;
}

export interface PeersUpdatedEvent {
  roomId: string;
  snapshotSeq: number;
  peers: PeerInfo[];
}

export interface RouteDiagnosticEntry {
  ipv4: string;
  hostname: string;
  nextHopIpv4: string;
  nextHopHostname: string;
  pathLen: number;
  pathLatencyMs: number;
  version: string;
}

export interface PendingPairingRequest {
  requestId: string;
  pairingId: string;
  clientName: string;
  sourceAddress: string;
  valid: boolean;
  receivedAtMs: number;
}

export interface TrustedClient {
  clientId: string;
  name: string;
  enabled: boolean;
}

export interface DisplayOutputInfo {
  id: string;
  name: string;
  width: number;
  height: number;
  refreshHz: number;
  isPrimary: boolean;
  isVirtual: boolean;
}

export interface VirtualDisplayState {
  supportedDriverDetected: boolean;
  driverName: string | null;
  virtualOutputActive: boolean;
  managedByEasystream: boolean;
  hasPhysicalDisplay: boolean;
  guidance: string;
}

export interface HostStatus {
  enabled: boolean;
  sunshineInstalled: boolean;
  sunshineVersion: string | null;
  sunshineOwnedByEasystream: boolean;
  sunshineConflictDetected: boolean;
  sunshineConflictMessage: string | null;
  serviceRunning: boolean;
  managementPort: number;
  certFingerprint: string | null;
  activeInboundSessions: number;
  pendingPairings: PendingPairingRequest[];
  trustedClients: TrustedClient[];
  captureDisplays: DisplayOutputInfo[];
  virtualDisplay: VirtualDisplayState;
  firewallScoped: boolean;
  lastError: CommandError | null;
}

export type StreamPresetId =
  | "saver720p30"
  | "balanced1080p60"
  | "highRefresh1440p120";

export interface StreamPreset {
  id: StreamPresetId;
  label: string;
  description: string;
  width: number;
  height: number;
  fps: number;
  bitrateKbps: number;
  codec: string;
  displayMode: string;
}

export type SessionStage =
  | "idle"
  | "probingHost"
  | "pairingWaitingUser"
  | "listingApps"
  | "launchingPlayer"
  | "playerOpened"
  | "reconnecting"
  | "disconnecting"
  | "failed";

export interface SessionSnapshot {
  operationId: string | null;
  roomId: string | null;
  peerId: string | null;
  peerName: string | null;
  peerIp: string | null;
  stage: SessionStage;
  pinDisplay: string | null;
  preset: StreamPreset;
  selectedApp: string | null;
  routePath: RoutePath;
  downgradeHint: string | null;
  error: CommandError | null;
  startedAtMs: number | null;
  pairingWaitMs: number | null;
  playerPid: number | null;
}

export interface AppSettings {
  defaultPreset: StreamPresetId;
  displayMode: string;
  videoCodec: string;
  relayNodes: string[];
  rememberRoomSecret: boolean;
  deviceName: string;
  sunshineBasePort: number;
  easytierRpcPort: number;
  lastRoomName: string | null;
  selectedAppsByHost: Record<string, string>;
  trustedHostIdentities: Record<string, string>;
}

export interface ComponentStatusItem {
  id: string;
  name: string;
  pinnedVersion: string;
  license: string;
  found: boolean;
  path: string | null;
  note: string;
}

export interface FullSnapshot {
  appVersion: string;
  room: RoomSnapshot;
  peers: PeerInfo[];
  routes: RouteDiagnosticEntry[];
  snapshotSeq: number;
  host: HostStatus;
  session: SessionSnapshot;
  settings: AppSettings;
  presets: StreamPreset[];
  components: ComponentStatusItem[];
}

export interface JoinNetworkRequest {
  roomName: string;
  roomSecret: string;
  addressMode: AddressMode;
  relayNodes: string[];
  rememberSecret: boolean;
  deviceName?: string | null;
}

export interface ConnectPeerRequest {
  peerId: string;
  presetId: StreamPresetId;
  displayMode?: string | null;
  appName?: string | null;
  confirmIdentityChange?: boolean;
}

export interface ConnectPeerResponse {
  operationId: string;
  reusedExisting: boolean;
}

export interface DecidePairingRequest {
  requestId: string;
  approve: boolean;
  pin?: string | null;
  clientName?: string | null;
}

export interface DecidePairingResult {
  requestId: string;
  approved: boolean;
  status: boolean;
  message: string;
}

export interface GeneratedRoomCredentials {
  roomName: string;
  roomSecret: string;
}
