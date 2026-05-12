import { useState } from "react";
import {
  ArrowSync16Regular,
  Checkmark16Regular,
  Eye16Regular,
  EyeOff16Regular,
} from "@fluentui/react-icons";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import type { ProxyMode } from "@/hooks/useWebDAVSettings";

type ConnectionSectionProps = {
  enabled: boolean;
  setEnabled: (value: boolean) => void;
  url: string;
  setUrl: (value: string) => void;
  username: string;
  setUsername: (value: string) => void;
  password: string;
  setPassword: (value: string) => void;
  remoteDir: string;
  setRemoteDir: (value: string) => void;
  proxyMode: ProxyMode;
  setProxyMode: (value: ProxyMode) => void;
  proxyUrl: string;
  setProxyUrl: (value: string) => void;
  acceptInvalidCerts: boolean;
  setAcceptInvalidCerts: (value: boolean) => void;
  testing: boolean;
  onTestConnection: () => void;
};

export function ConnectionSection({
  enabled,
  setEnabled,
  url,
  setUrl,
  username,
  setUsername,
  password,
  setPassword,
  remoteDir,
  setRemoteDir,
  proxyMode,
  setProxyMode,
  proxyUrl,
  setProxyUrl,
  acceptInvalidCerts,
  setAcceptInvalidCerts,
  testing,
  onTestConnection,
}: ConnectionSectionProps) {
  const [showPassword, setShowPassword] = useState(false);

  return (
    <div className="rounded-lg border bg-card p-4">
      <h3 className="text-sm font-medium mb-3">WebDAV 同步</h3>
      <p className="text-xs text-muted-foreground mb-4">
        通过 WebDAV 在多台设备间同步剪贴板数据
      </p>
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <div className="space-y-0.5">
            <Label className="text-xs">启用同步</Label>
            <p className="text-xs text-muted-foreground">
              开启后将允许与 WebDAV 服务器同步数据
            </p>
          </div>
          <Switch checked={enabled} onCheckedChange={setEnabled} />
        </div>

        {enabled && (
          <>
            <div className="space-y-2 pt-1">
              <div className="space-y-1.5">
                <Label className="text-xs">WebDAV 地址</Label>
                <Input
                  className="h-8 text-xs"
                  placeholder="https://dav.example.com"
                  value={url}
                  onChange={(e) => setUrl(e.target.value)}
                />
              </div>
              <div className="grid grid-cols-2 gap-2">
                <div className="space-y-1.5">
                  <Label className="text-xs">用户名</Label>
                  <Input
                    className="h-8 text-xs"
                    placeholder="username"
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                  />
                </div>
                <div className="space-y-1.5">
                  <Label className="text-xs">密码</Label>
                  <div className="relative">
                    <Input
                      className="h-8 text-xs pr-8"
                      type={showPassword ? "text" : "password"}
                      placeholder="password"
                      value={password}
                      onChange={(e) => setPassword(e.target.value)}
                    />
                    <button
                      type="button"
                      className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground transition-colors"
                      onClick={() => setShowPassword(!showPassword)}
                    >
                      {showPassword ? <EyeOff16Regular className="w-3.5 h-3.5" /> : <Eye16Regular className="w-3.5 h-3.5" />}
                    </button>
                  </div>
                </div>
              </div>
              <div className="space-y-1.5">
                <Label className="text-xs">远端目录</Label>
                <Input
                  className="h-8 text-xs"
                  placeholder="/elegant-clipboard"
                  value={remoteDir}
                  onChange={(e) => setRemoteDir(e.target.value)}
                />
              </div>
              <div className="space-y-1.5">
                <Label className="text-xs">网络代理</Label>
                <div className="flex items-center gap-2">
                  <Select value={proxyMode} onValueChange={(value) => setProxyMode(value as ProxyMode)}>
                    <SelectTrigger className="w-[130px] h-8 text-xs shrink-0"><SelectValue /></SelectTrigger>
                    <SelectContent>
                      <SelectItem value="system">系统代理</SelectItem>
                      <SelectItem value="none">不使用代理</SelectItem>
                      <SelectItem value="custom">自定义代理</SelectItem>
                    </SelectContent>
                  </Select>
                  {proxyMode === "custom" && (
                    <Input
                      className="h-8 text-xs flex-1"
                      placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
                      value={proxyUrl}
                      onChange={(e) => setProxyUrl(e.target.value)}
                    />
                  )}
                </div>
              </div>
              <div className="flex items-center justify-between rounded-md border border-amber-500/30 bg-amber-500/5 p-3">
                <div className="space-y-0.5 pr-4">
                  <Label className="text-xs">接受无效证书</Label>
                  <p className="text-xs text-muted-foreground">
                    仅在连接自签名或内网 WebDAV 服务时启用；开启后会跳过 TLS 证书校验。
                  </p>
                </div>
                <Switch checked={acceptInvalidCerts} onCheckedChange={setAcceptInvalidCerts} />
              </div>
            </div>

            <div className="flex items-center gap-2 pt-1">
              <Button
                variant="outline"
                size="sm"
                className="h-7 text-xs"
                onClick={onTestConnection}
                disabled={testing || !url}
              >
                {testing ? (
                  <ArrowSync16Regular className="w-3.5 h-3.5 mr-1 animate-spin" />
                ) : (
                  <Checkmark16Regular className="w-3.5 h-3.5 mr-1" />
                )}
                测试连接
              </Button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}