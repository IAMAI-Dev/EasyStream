import { useCallback, useEffect, useState } from "react";
import {
  AlertTriangle,
  KeyRound,
  Loader2,
  MonitorPlay,
  X,
} from "lucide-react";
import { api, subscribeEvents } from "./api";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { HostPanel } from "./components/HostPanel";
import { PeerCard } from "./components/PeerCard";
import { RoomBar } from "./components/RoomBar";
import { SettingsPanel } from "./components/SettingsPanel";
import { Sidebar, type NavTab } from "./components/Sidebar";
import { TrustedClientsPanel } from "./components/TrustedClientsPanel";
import type {
  AddressMode,
  AppSettings,
  CommandError,
  DecidePairingRequest,
  FullSnapshot,
  StreamPresetId,
} from "./types";

function parseInvokeError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    const e = err as CommandError;
    return e.detail ? `${e.message} (${e.detail})` : e.message;
  }
  return String(err);
}

export function App() {
  const [snapshot, setSnapshot] = useState<FullSnapshot | null>(null);
  const [activeTab, setActiveTab] = useState<NavTab>("lobby");
  const [busy, setBusy] = useState(false);
  const [bannerError, setBannerError] = useState<string | null>(null);
  const [logs, setLogs] = useState<string[]>([]);

  const loadInitial = useCallback(async () => {
    try {
      const snap = await api.getSnapshot();
      setSnapshot(snap);
      const recentLogs = await api.getLogs();
      setLogs(recentLogs);
    } catch (err) {
      setBannerError(parseInvokeError(err));
    }
  }, []);

  useEffect(() => {
    loadInitial();
    let unlisteners: (() => void)[] = [];

    subscribeEvents({
      onRoomUpdated: (room) =>
        setSnapshot((prev) => (prev ? { ...prev, room } : prev)),
      onPeersUpdated: (ev) =>
        setSnapshot((prev) =>
          prev
            ? {
                ...prev,
                peers: ev.peers,
                snapshotSeq: ev.snapshotSeq,
              }
            : prev
        ),
      onHostUpdated: (host) =>
        setSnapshot((prev) => (prev ? { ...prev, host } : prev)),
      onSessionUpdated: (session) =>
        setSnapshot((prev) => (prev ? { ...prev, session } : prev)),
      onSnapshotUpdated: (full) => setSnapshot(full),
    }).then((fns) => {
      unlisteners = fns;
    });

    return () => {
      unlisteners.forEach((fn) => fn());
    };
  }, [loadInitial]);

  const runAction = async (fn: () => Promise<void>) => {
    setBusy(true);
    setBannerError(null);
    try {
      await fn();
    } catch (err) {
      setBannerError(parseInvokeError(err));
    } finally {
      setBusy(false);
    }
  };

  if (!snapshot) {
    return (
      <div className="h-screen bg-surface-950 flex items-center justify-center text-slate-400 gap-2 text-sm">
        <Loader2 className="w-4 h-4 animate-spin text-sky-400" />
        正在初始化 EasyStream...
      </div>
    );
  }

  const { room, peers, routes, host, session, settings, presets, components } =
    snapshot;

  const handleJoin = async (
    roomName: string,
    roomSecret: string,
    addressMode: AddressMode,
    rememberSecret: boolean
  ) => {
    await runAction(async () => {
      const updated = await api.joinNetwork({
        roomName,
        roomSecret,
        addressMode,
        relayNodes: settings.relayNodes,
        rememberSecret,
        deviceName: settings.deviceName,
      });
      setSnapshot((prev) => (prev ? { ...prev, room: updated } : prev));
    });
  };

  const handleLeave = async () => {
    await runAction(async () => {
      await api.leaveNetwork();
    });
  };

  const handleConnect = async (
    peerId: string,
    presetId: StreamPresetId,
    appName: string | null,
    confirmIdentityChange = false
  ) => {
    await runAction(async () => {
      await api.connectToPeer({
        peerId,
        presetId,
        displayMode: settings.displayMode,
        appName,
        confirmIdentityChange,
      });
    });
  };

  const handleDisconnect = async (operationId?: string | null) => {
    await runAction(async () => {
      await api.disconnect(operationId);
    });
  };

  const handleToggleHost = async (enable: boolean) => {
    await runAction(async () => {
      const updated = await api.toggleHostService(enable);
      setSnapshot((prev) => (prev ? { ...prev, host: updated } : prev));
    });
  };

  const handleDecidePairing = async (req: DecidePairingRequest) => {
    await runAction(async () => {
      await api.decidePairing(req);
    });
  };

  const handleRefreshClients = async () => {
    await runAction(async () => {
      const clients = await api.listTrustedClients();
      setSnapshot((prev) =>
        prev ? { ...prev, host: { ...prev.host, trustedClients: clients } } : prev
      );
    });
  };

  const handleRevokeClient = async (clientId: string) => {
    await runAction(async () => {
      await api.revokeClient(clientId);
    });
  };

  const handleSaveSettings = async (next: AppSettings) => {
    await runAction(async () => {
      const saved = await api.updateSettings(next);
      setSnapshot((prev) => (prev ? { ...prev, settings: saved } : prev));
    });
  };

  const handleRefreshLogs = async () => {
    const nextLogs = await api.getLogs();
    setLogs(nextLogs);
  };

  return (
    <div className="h-screen flex bg-surface-950 text-slate-100">
      <Sidebar
        activeTab={activeTab}
        onSelectTab={setActiveTab}
        room={room}
        host={host}
        peerCount={peers.length}
      />

      <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {/* Global Error / Session Status Bar */}
        <div className="px-6 pt-5 space-y-3 shrink-0">
          {bannerError && (
            <div className="bg-rose-500/15 border border-rose-500/30 rounded-xl px-4 py-3 flex items-center justify-between gap-3 text-xs text-rose-200">
              <div className="flex items-center gap-2">
                <AlertTriangle className="w-4 h-4 text-rose-400 shrink-0" />
                <span>{bannerError}</span>
              </div>
              <button
                type="button"
                onClick={() => setBannerError(null)}
                className="text-rose-300 hover:text-white"
              >
                <X className="w-4 h-4" />
              </button>
            </div>
          )}

          {/* Active Outbound Session Banner */}
          {session.stage !== "idle" && (
            <div className="bg-sky-500/10 border border-sky-500/30 rounded-xl px-4 py-3 flex flex-wrap items-center justify-between gap-3">
              <div className="flex items-center gap-3">
                {session.stage === "pairingWaitingUser" ? (
                  <KeyRound className="w-5 h-5 text-amber-400 shrink-0" />
                ) : session.stage === "playerOpened" ? (
                  <MonitorPlay className="w-5 h-5 text-emerald-400 shrink-0" />
                ) : session.stage === "failed" ? (
                  <AlertTriangle className="w-5 h-5 text-rose-400 shrink-0" />
                ) : (
                  <Loader2 className="w-5 h-5 text-sky-400 animate-spin shrink-0" />
                )}

                <div className="text-xs space-y-0.5">
                  <div className="font-semibold text-slate-100">
                    {session.stage === "probingHost" &&
                      `正在探测主机 ${session.peerName ?? ""}...`}
                    {session.stage === "pairingWaitingUser" &&
                      `首次配对等待被控端确认 — 请在对方 EasyStream 窗口录入短码`}
                    {session.stage === "listingApps" &&
                      `正在获取 ${session.peerName ?? ""} 的可串流应用列表...`}
                    {session.stage === "launchingPlayer" &&
                      `启动中：正在打开 Moonlight 播放器 [${session.selectedApp ?? "Desktop"}]...`}
                    {session.stage === "playerOpened" &&
                      `已打开播放器 (${session.preset.width}x${session.preset.height}@${session.preset.fps}fps) → ${session.peerName ?? ""}`}
                    {session.stage === "disconnecting" && "正在断开串流会话..."}
                    {session.stage === "failed" &&
                      `连接失败：${session.error?.message ?? "未知错误"}`}
                  </div>

                  {session.pinDisplay && (
                    <div className="flex items-center gap-2 pt-1">
                      <span className="text-amber-300">本次配对短码：</span>
                      <span className="px-3 py-0.5 rounded-md bg-amber-500/20 border border-amber-500/40 text-lg font-mono font-bold tracking-[0.3em] text-amber-200">
                        {session.pinDisplay}
                      </span>
                    </div>
                  )}

                  {session.downgradeHint && (
                    <div className="text-amber-300">{session.downgradeHint}</div>
                  )}
                </div>
              </div>

              <button
                type="button"
                onClick={() => handleDisconnect(session.operationId)}
                className="px-3 py-1.5 rounded-lg bg-surface-800 hover:bg-surface-700 border border-slate-700 text-xs font-medium text-slate-200"
              >
                {session.stage === "failed" ? "关闭提示" : "取消 / 断开"}
              </button>
            </div>
          )}
        </div>

        {/* Scrollable Workspace Area */}
        <div className="flex-1 overflow-y-auto px-6 py-5 space-y-5">
          {activeTab === "lobby" && (
            <>
              <RoomBar
                room={room}
                defaultRelayNodes={settings.relayNodes}
                deviceName={settings.deviceName}
                onJoin={handleJoin}
                onLeave={handleLeave}
                onGenerate={api.generateRoomCredentials}
                onLoadSaved={api.loadSavedRoom}
                busy={busy}
              />

              <section className="space-y-3">
                <div className="flex items-center justify-between">
                  <h2 className="text-sm font-semibold text-slate-200">
                    房间设备列表 ({peers.length})
                  </h2>
                  <span className="text-xs text-slate-500">
                    每 2 秒同步路由路径与主机可达状态
                  </span>
                </div>

                {room.stage !== "inRoom" ? (
                  <div className="bg-surface-900/60 border border-dashed border-slate-800 rounded-xl p-10 text-center text-sm text-slate-500">
                    请先创建或加入一个虚拟组网房间，随后即可在此发现同房间设备并一键发起串流。
                  </div>
                ) : peers.length === 0 ? (
                  <div className="bg-surface-900/60 border border-dashed border-slate-800 rounded-xl p-10 text-center text-sm text-slate-500">
                    已加入房间 <span className="font-mono text-slate-300">{room.roomName}</span>，正在等待其他设备加入...
                  </div>
                ) : (
                  <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                    {peers.map((peer) => (
                      <PeerCard
                        key={peer.peerId}
                        peer={peer}
                        presets={presets}
                        defaultPresetId={settings.defaultPreset}
                        session={session}
                        onConnect={handleConnect}
                        onDisconnect={handleDisconnect}
                      />
                    ))}
                  </div>
                )}
              </section>
            </>
          )}

          {activeTab === "host" && (
            <HostPanel
              host={host}
              onToggleHost={handleToggleHost}
              onDecidePairing={handleDecidePairing}
              busy={busy}
            />
          )}

          {activeTab === "trusted" && (
            <TrustedClientsPanel
              clients={host.trustedClients}
              hostEnabled={host.enabled}
              onRefresh={handleRefreshClients}
              onRevoke={handleRevokeClient}
              busy={busy}
            />
          )}

          {activeTab === "diagnostics" && (
            <DiagnosticsPanel
              components={components}
              routes={routes}
              logs={logs}
              onRefreshLogs={handleRefreshLogs}
              onExportDiagnostics={api.exportDiagnostics}
            />
          )}

          {activeTab === "settings" && (
            <SettingsPanel
              settings={settings}
              presets={presets}
              onSave={handleSaveSettings}
              busy={busy}
            />
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
