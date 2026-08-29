import { useState } from "react";
import { useAppStore } from "../store";
import { PROTOCOLS } from "../types";
import type { Protocol, ServerConfig } from "../types";

const PROTOCOL_FIELDS: Record<Protocol, { uuid: boolean; password: boolean; encryption: boolean; flow: boolean }> = {
  vless: { uuid: true, password: false, encryption: true, flow: true },
  vmess: { uuid: true, password: false, encryption: true, flow: false },
  trojan: { uuid: false, password: true, encryption: false, flow: false },
  shadowsocks: { uuid: false, password: true, encryption: true, flow: false },
  wireguard: { uuid: false, password: false, encryption: false, flow: false },
};

// WireGuard has its own crypto handshake and no TLS layer at all (see
// core-manager::config::build_outbound's wireguard arm) -- the shared TLS
// fieldset below doesn't apply to it.
const PROTOCOLS_WITH_TLS: Protocol[] = ["vless", "trojan", "shadowsocks", "vmess"];

function newId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `srv-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function ServerForm({ onDone }: { onDone: () => void }) {
  const addServer = useAppStore((s) => s.addServer);

  const [name, setName] = useState("");
  const [protocol, setProtocol] = useState<Protocol>("vless");
  const [address, setAddress] = useState("");
  const [port, setPort] = useState("443");
  const [uuid, setUuid] = useState("");
  const [password, setPassword] = useState("");
  const [encryption, setEncryption] = useState("");
  const [flow, setFlow] = useState("");

  const [wireguardPrivateKey, setWireguardPrivateKey] = useState("");
  const [wireguardPeerPublicKey, setWireguardPeerPublicKey] = useState("");
  const [wireguardPreSharedKey, setWireguardPreSharedKey] = useState("");
  const [wireguardLocalAddress, setWireguardLocalAddress] = useState("");

  const [tlsEnabled, setTlsEnabled] = useState(true);
  const [serverName, setServerName] = useState("");
  const [insecure, setInsecure] = useState(false);
  const [realityPublicKey, setRealityPublicKey] = useState("");
  const [realityShortId, setRealityShortId] = useState("");

  const [submitting, setSubmitting] = useState(false);

  const fields = PROTOCOL_FIELDS[protocol];
  const isWireguard = protocol === "wireguard";
  const showTls = PROTOCOLS_WITH_TLS.includes(protocol);

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    const portNum = Number(port);
    if (!name.trim() || !address.trim() || !Number.isInteger(portNum) || portNum <= 0) {
      return;
    }

    const server: ServerConfig = {
      id: newId(),
      name: name.trim(),
      protocol,
      address: address.trim(),
      port: portNum,
      uuid: fields.uuid ? uuid.trim() || null : null,
      password: fields.password ? password || null : null,
      encryption: fields.encryption ? encryption.trim() || null : null,
      flow: fields.flow ? flow.trim() || null : null,
      tls:
        showTls && tlsEnabled
          ? {
              enabled: true,
              serverName: serverName.trim() || null,
              insecure,
              realityPublicKey: realityPublicKey.trim() || null,
              realityShortId: realityShortId.trim() || null,
            }
          : null,
      wireguardPrivateKey: isWireguard ? wireguardPrivateKey.trim() || null : null,
      wireguardPeerPublicKey: isWireguard ? wireguardPeerPublicKey.trim() || null : null,
      wireguardPreSharedKey: isWireguard ? wireguardPreSharedKey.trim() || null : null,
      wireguardLocalAddress: isWireguard ? wireguardLocalAddress.trim() || null : null,
    };

    setSubmitting(true);
    try {
      await addServer(server);
      onDone();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="flex flex-col gap-4 rounded-xl bg-white p-5 shadow dark:bg-slate-800"
    >
      <h3 className="text-base font-semibold">Add server</h3>

      <div className="grid grid-cols-2 gap-3">
        <label className="flex flex-col gap-1 text-sm">
          Name
          <input
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          />
        </label>

        <label className="flex flex-col gap-1 text-sm">
          Protocol
          <select
            value={protocol}
            onChange={(e) => setProtocol(e.target.value as Protocol)}
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          >
            {PROTOCOLS.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        </label>

        <label className="flex flex-col gap-1 text-sm">
          Address
          <input
            required
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="example.com"
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          />
        </label>

        <label className="flex flex-col gap-1 text-sm">
          Port
          <input
            required
            type="number"
            min={1}
            max={65535}
            value={port}
            onChange={(e) => setPort(e.target.value)}
            className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
          />
        </label>

        {fields.uuid && (
          <label className="flex flex-col gap-1 text-sm">
            UUID
            <input
              value={uuid}
              onChange={(e) => setUuid(e.target.value)}
              className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
            />
          </label>
        )}

        {fields.password && (
          <label className="flex flex-col gap-1 text-sm">
            Password
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
            />
          </label>
        )}

        {fields.encryption && (
          <label className="flex flex-col gap-1 text-sm">
            {protocol === "shadowsocks" ? "Cipher" : "Encryption"}
            <input
              value={encryption}
              onChange={(e) => setEncryption(e.target.value)}
              placeholder={protocol === "shadowsocks" ? "aes-256-gcm" : "auto"}
              className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
            />
          </label>
        )}

        {fields.flow && (
          <label className="flex flex-col gap-1 text-sm">
            Flow
            <input
              value={flow}
              onChange={(e) => setFlow(e.target.value)}
              placeholder="xtls-rprx-vision"
              className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
            />
          </label>
        )}

        {isWireguard && (
          <>
            <label className="flex flex-col gap-1 text-sm">
              Private key
              <input
                value={wireguardPrivateKey}
                onChange={(e) => setWireguardPrivateKey(e.target.value)}
                placeholder="base64 32-byte key"
                className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
              />
            </label>

            <label className="flex flex-col gap-1 text-sm">
              Peer public key
              <input
                value={wireguardPeerPublicKey}
                onChange={(e) => setWireguardPeerPublicKey(e.target.value)}
                placeholder="base64 32-byte key"
                className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
              />
            </label>

            <label className="flex flex-col gap-1 text-sm">
              Pre-shared key (optional)
              <input
                value={wireguardPreSharedKey}
                onChange={(e) => setWireguardPreSharedKey(e.target.value)}
                placeholder="base64 32-byte key"
                className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
              />
            </label>

            <label className="flex flex-col gap-1 text-sm">
              Local address
              <input
                value={wireguardLocalAddress}
                onChange={(e) => setWireguardLocalAddress(e.target.value)}
                placeholder="10.0.0.2/32"
                className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
              />
            </label>
          </>
        )}
      </div>

      {showTls && (
        <fieldset className="rounded-lg border border-slate-200 p-3 dark:border-slate-700">
          <legend className="px-1 text-sm font-medium">TLS</legend>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={tlsEnabled}
              onChange={(e) => setTlsEnabled(e.target.checked)}
            />
            Enabled
          </label>

          {tlsEnabled && (
            <div className="mt-3 grid grid-cols-2 gap-3">
              <label className="flex flex-col gap-1 text-sm">
                Server name (SNI)
                <input
                  value={serverName}
                  onChange={(e) => setServerName(e.target.value)}
                  className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
                />
              </label>

              <label className="mt-6 flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={insecure}
                  onChange={(e) => setInsecure(e.target.checked)}
                />
                Allow insecure (skip cert verify)
              </label>

              <label className="flex flex-col gap-1 text-sm">
                Reality public key
                <input
                  value={realityPublicKey}
                  onChange={(e) => setRealityPublicKey(e.target.value)}
                  className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
                />
              </label>

              <label className="flex flex-col gap-1 text-sm">
                Reality short ID
                <input
                  value={realityShortId}
                  onChange={(e) => setRealityShortId(e.target.value)}
                  className="rounded-md border border-slate-300 px-3 py-1.5 dark:border-slate-600 dark:bg-slate-900"
                />
              </label>
            </div>
          )}
        </fieldset>
      )}

      <div className="flex justify-end gap-2">
        <button
          type="button"
          onClick={onDone}
          className="rounded-md px-4 py-2 text-sm text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-700"
        >
          Cancel
        </button>
        <button
          type="submit"
          disabled={submitting}
          className="rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
        >
          Add server
        </button>
      </div>
    </form>
  );
}
