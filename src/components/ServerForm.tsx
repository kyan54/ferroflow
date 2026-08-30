import { useState } from "react";
import { useAppStore } from "../store";
import { useTranslation } from "../i18n";
import { PROTOCOLS } from "../types";
import type { Protocol, ServerConfig } from "../types";
import { Card, CardHeader, CardTitle, CardContent, Button, Input, Select, Toggle } from "./ui";
import { newId } from "../lib/utils";

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

export function ServerForm({
  onDone,
  initialServer,
}: {
  onDone: () => void;
  /** When present, the form edits this server in place (via `updateServer`)
   * instead of creating a new one -- preserves `id`/`source` exactly,
   * mirroring `RuleForm`'s `initialRule` edit pattern. */
  initialServer?: ServerConfig;
}) {
  const { t } = useTranslation();
  const addServer = useAppStore((s) => s.addServer);
  const updateServer = useAppStore((s) => s.updateServer);
  const isEditing = !!initialServer;

  const [name, setName] = useState(initialServer?.name ?? "");
  const [protocol, setProtocol] = useState<Protocol>(initialServer?.protocol ?? "vless");
  const [address, setAddress] = useState(initialServer?.address ?? "");
  const [port, setPort] = useState(initialServer ? String(initialServer.port) : "443");
  const [uuid, setUuid] = useState(initialServer?.uuid ?? "");
  const [password, setPassword] = useState(initialServer?.password ?? "");
  const [encryption, setEncryption] = useState(initialServer?.encryption ?? "");
  const [flow, setFlow] = useState(initialServer?.flow ?? "");

  const [wireguardPrivateKey, setWireguardPrivateKey] = useState(initialServer?.wireguardPrivateKey ?? "");
  const [wireguardPeerPublicKey, setWireguardPeerPublicKey] = useState(
    initialServer?.wireguardPeerPublicKey ?? "",
  );
  const [wireguardPreSharedKey, setWireguardPreSharedKey] = useState(
    initialServer?.wireguardPreSharedKey ?? "",
  );
  const [wireguardLocalAddress, setWireguardLocalAddress] = useState(
    initialServer?.wireguardLocalAddress ?? "",
  );

  const [tlsEnabled, setTlsEnabled] = useState(initialServer ? !!initialServer.tls?.enabled : true);
  const [serverName, setServerName] = useState(initialServer?.tls?.serverName ?? "");
  const [insecure, setInsecure] = useState(initialServer?.tls?.insecure ?? false);
  const [realityPublicKey, setRealityPublicKey] = useState(initialServer?.tls?.realityPublicKey ?? "");
  const [realityShortId, setRealityShortId] = useState(initialServer?.tls?.realityShortId ?? "");

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
      id: initialServer?.id ?? newId(),
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
      source: initialServer?.source ?? "manual",
    };

    setSubmitting(true);
    try {
      if (isEditing) {
        await updateServer(server);
      } else {
        await addServer(server);
      }
      onDone();
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Card>
      <form onSubmit={handleSubmit}>
        <CardHeader>
          <CardTitle>{isEditing ? t.serverForm.editTitle : t.serverForm.title}</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-4 pt-4">
          <div className="grid grid-cols-2 gap-3">
            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              {t.serverForm.name}
              <Input required value={name} onChange={(e) => setName(e.target.value)} />
            </label>

            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              {t.serverForm.protocol}
              <Select value={protocol} onChange={(e) => setProtocol(e.target.value as Protocol)}>
                {PROTOCOLS.map((p) => (
                  <option key={p} value={p}>
                    {p}
                  </option>
                ))}
              </Select>
            </label>

            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              {t.serverForm.address}
              <Input
                required
                value={address}
                onChange={(e) => setAddress(e.target.value)}
                placeholder="example.com"
              />
            </label>

            <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
              {t.serverForm.port}
              <Input
                required
                type="number"
                min={1}
                max={65535}
                value={port}
                onChange={(e) => setPort(e.target.value)}
              />
            </label>

            {fields.uuid && (
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.serverForm.uuid}
                <Input value={uuid} onChange={(e) => setUuid(e.target.value)} />
              </label>
            )}

            {fields.password && (
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.serverForm.password}
                <Input type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
              </label>
            )}

            {fields.encryption && (
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {protocol === "shadowsocks" ? t.serverForm.cipher : t.serverForm.encryption}
                <Input
                  value={encryption}
                  onChange={(e) => setEncryption(e.target.value)}
                  placeholder={protocol === "shadowsocks" ? "aes-256-gcm" : "auto"}
                />
              </label>
            )}

            {fields.flow && (
              <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                {t.serverForm.flow}
                <Input
                  value={flow}
                  onChange={(e) => setFlow(e.target.value)}
                  placeholder="xtls-rprx-vision"
                />
              </label>
            )}

            {isWireguard && (
              <>
                <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                  {t.serverForm.wireguardPrivateKey}
                  <Input
                    value={wireguardPrivateKey}
                    onChange={(e) => setWireguardPrivateKey(e.target.value)}
                    placeholder="base64 32-byte key"
                  />
                </label>

                <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                  {t.serverForm.wireguardPeerPublicKey}
                  <Input
                    value={wireguardPeerPublicKey}
                    onChange={(e) => setWireguardPeerPublicKey(e.target.value)}
                    placeholder="base64 32-byte key"
                  />
                </label>

                <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                  {t.serverForm.wireguardPreSharedKey}
                  <Input
                    value={wireguardPreSharedKey}
                    onChange={(e) => setWireguardPreSharedKey(e.target.value)}
                    placeholder="base64 32-byte key"
                  />
                </label>

                <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                  {t.serverForm.wireguardLocalAddress}
                  <Input
                    value={wireguardLocalAddress}
                    onChange={(e) => setWireguardLocalAddress(e.target.value)}
                    placeholder="10.0.0.2/32"
                  />
                </label>
              </>
            )}
          </div>

          {showTls && (
            <fieldset className="rounded-lg border border-line p-3">
              <legend className="px-1 text-sm font-medium text-fg-dim">{t.serverForm.tlsLegend}</legend>
              <Toggle checked={tlsEnabled} onChange={setTlsEnabled} label={t.serverForm.tlsEnabled} />

              {tlsEnabled && (
                <div className="mt-3 grid grid-cols-2 gap-3">
                  <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                    {t.serverForm.tlsServerName}
                    <Input value={serverName} onChange={(e) => setServerName(e.target.value)} />
                  </label>

                  <div className="flex items-end pb-1.5">
                    <Toggle checked={insecure} onChange={setInsecure} label={t.serverForm.tlsInsecure} />
                  </div>

                  <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                    {t.serverForm.tlsRealityPublicKey}
                    <Input value={realityPublicKey} onChange={(e) => setRealityPublicKey(e.target.value)} />
                  </label>

                  <label className="flex flex-col gap-1 text-sm font-medium text-fg-dim">
                    {t.serverForm.tlsRealityShortId}
                    <Input value={realityShortId} onChange={(e) => setRealityShortId(e.target.value)} />
                  </label>
                </div>
              )}
            </fieldset>
          )}

          <div className="flex justify-end gap-2">
            <Button type="button" variant="ghost" onClick={onDone}>
              {t.serverForm.cancel}
            </Button>
            <Button type="submit" busy={submitting}>
              {isEditing ? t.serverForm.save : t.serverForm.submit}
            </Button>
          </div>
        </CardContent>
      </form>
    </Card>
  );
}
