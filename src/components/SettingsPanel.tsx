import React, { useState } from "react";
import { Save } from "lucide-react";
import type { AppSettings, StreamPreset, StreamPresetId } from "../types";

interface SettingsPanelProps {
  settings: AppSettings;
  presets: StreamPreset[];
  onSave: (settings: AppSettings) => Promise<void>;
  busy: boolean;
}

export const SettingsPanel: React.FC<SettingsPanelProps> = ({
  settings,
  presets,
  onSave,
  busy,
}) => {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [relayText, setRelayText] = useState(settings.relayNodes.join("\n"));
  const [savedNotice, setSavedNotice] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const relayNodes = relayText
      .split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean);
    await onSave({ ...draft, relayNodes });
    setSavedNotice(true);
    setTimeout(() => setSavedNotice(false), 2000);
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="bg-surface-900 border border-slate-800 rounded-xl p-5 space-y-5"
    >
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-base font-semibold text-slate-100">偏好设置</h2>
          <p className="text-xs text-slate-400">
            配置默认串流预设、显示模式、编码器基线与 EasyTier 接入/中继节点
          </p>
        </div>

        <button
          type="submit"
          disabled={busy}
          className="px-4 py-2 rounded-lg bg-sky-600 hover:bg-sky-500 text-white text-sm font-medium inline-flex items-center gap-2 transition-colors disabled:opacity-50"
        >
          <Save className="w-4 h-4" />
          {savedNotice ? "已保存" : "保存设置"}
        </button>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
        <div>
          <label className="block text-xs text-slate-400 mb-1">
            本机设备显示名称
          </label>
          <input
            type="text"
            value={draft.deviceName}
            onChange={(e) =>
              setDraft({ ...draft, deviceName: e.target.value })
            }
            className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-slate-100"
          />
        </div>

        <div>
          <label className="block text-xs text-slate-400 mb-1">
            默认画质预设
          </label>
          <select
            value={draft.defaultPreset}
            onChange={(e) =>
              setDraft({
                ...draft,
                defaultPreset: e.target.value as StreamPresetId,
              })
            }
            className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-slate-100"
          >
            {presets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label} — {p.description}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="block text-xs text-slate-400 mb-1">
            Moonlight 窗口模式
          </label>
          <select
            value={draft.displayMode}
            onChange={(e) =>
              setDraft({ ...draft, displayMode: e.target.value })
            }
            className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-slate-100"
          >
            <option value="fullscreen">全屏 (fullscreen)</option>
            <option value="borderless">无边框全屏 (borderless)</option>
            <option value="windowed">窗口化 (windowed)</option>
          </select>
        </div>

        <div>
          <label className="block text-xs text-slate-400 mb-1">
            视频编码基线（首轮推荐 H.264）
          </label>
          <select
            value={draft.videoCodec}
            onChange={(e) =>
              setDraft({ ...draft, videoCodec: e.target.value })
            }
            className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-slate-100"
          >
            <option value="H.264">H.264 (基线兼容性最佳)</option>
            <option value="HEVC">HEVC (H.265)</option>
            <option value="AV1">AV1</option>
            <option value="auto">auto (自动协商)</option>
          </select>
        </div>

        <div>
          <label className="block text-xs text-slate-400 mb-1">
            Sunshine 基准端口
          </label>
          <input
            type="number"
            value={draft.sunshineBasePort}
            onChange={(e) =>
              setDraft({
                ...draft,
                sunshineBasePort: Number(e.target.value) || 47989,
              })
            }
            className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-slate-100 font-mono"
          />
        </div>

        <div>
          <label className="block text-xs text-slate-400 mb-1">
            EasyTier 回环 RPC 端口
          </label>
          <input
            type="number"
            value={draft.easytierRpcPort}
            onChange={(e) =>
              setDraft({
                ...draft,
                easytierRpcPort: Number(e.target.value) || 15888,
              })
            }
            className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-slate-100 font-mono"
          />
        </div>
      </div>

      <div>
        <label className="block text-xs text-slate-400 mb-1">
          EasyTier 接入 / 中继节点（每行一个 URI）
        </label>
        <textarea
          rows={3}
          value={relayText}
          onChange={(e) => setRelayText(e.target.value)}
          placeholder="tcp://public.easytier.cn:11010"
          className="w-full px-3 py-2 rounded-lg bg-surface-950 border border-slate-800 text-xs font-mono text-slate-100"
        />
      </div>
    </form>
  );
};
