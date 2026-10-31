import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Notice } from "@/components/common/Notice";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { describeError } from "@/lib/errors";
import { locale, t, tCount } from "@/lib/i18n";
import { type AgentLogEntry, api } from "@/lib/tauri";

/** How many log entries the page shows. */
const LOG_SHOWN = 50;

const toolLabels: Record<string, Parameters<typeof t>[0]> = {
  list_accounts: "agents.tool.listAccounts",
  list_folders: "agents.tool.listFolders",
  list_messages: "agents.tool.listMessages",
  search_messages: "agents.tool.searchMessages",
  read_message: "agents.tool.readMessage",
};

/** A shell word, quoted when it has spaces or quotes in it. */
function shellWord(word: string): string {
  return /^[\w./:\\-]+$/.test(word) ? word : `"${word.replace(/(["\\$`])/g, "\\$1")}"`;
}

function CopyBlock({ label, text }: { label: string; text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="grid gap-1.5">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-medium">{label}</span>
        <Button
          variant="ghost"
          size="sm"
          onClick={() =>
            void navigator.clipboard.writeText(text).then(
              () => setCopied(true),
              () => setCopied(false),
            )
          }
        >
          <Icon name={copied ? "check" : "restore"} />
          {copied ? t("agents.copied") : t("agents.copy")}
        </Button>
      </div>
      <pre className="overflow-x-auto rounded-lg bg-muted px-3 py-2 font-mono text-xs leading-relaxed break-all whitespace-pre-wrap select-all">
        {text}
      </pre>
    </div>
  );
}

function LogEntry({ entry }: { entry: AgentLogEntry }) {
  const label = toolLabels[entry.tool];
  const time = new Date(entry.at).toLocaleString(locale, {
    dateStyle: "short",
    timeStyle: "short",
  });
  return (
    <li className="grid gap-0.5 py-2 text-sm">
      <div className="flex items-center gap-2">
        <span className="font-medium">{label ? t(label) : entry.tool}</span>
        {entry.refused ? (
          <span className="text-xs font-medium text-destructive">{t("agents.log.refused")}</span>
        ) : (
          <span className="text-xs text-muted-foreground">
            {tCount("agents.log.results", entry.results)}
          </span>
        )}
        <span className="ml-auto shrink-0 text-xs text-muted-foreground tabular-nums">{time}</span>
      </div>
      <div className="flex min-w-0 gap-2 text-xs text-muted-foreground">
        {entry.client && <span className="shrink-0">{entry.client}</span>}
        {entry.detail && <span className="truncate">„{entry.detail}“</span>}
      </div>
    </li>
  );
}

/** Which accounts AI agents may read over MCP, how to connect one, and what they read. */
export function AgentSettings() {
  const queryClient = useQueryClient();
  const settings = useQuery({ queryKey: ["agent-settings"], queryFn: api.getAgentSettings });
  const launch = useQuery({ queryKey: ["agent-launch"], queryFn: api.agentLaunch });
  // The server runs as its own process; the page looks again while it is open.
  const log = useQuery({
    queryKey: ["agent-log"],
    queryFn: () => api.listAgentLog(LOG_SHOWN),
    refetchInterval: 5000,
  });
  const refresh = () => queryClient.invalidateQueries({ queryKey: ["agent-settings"] });
  const setEnabled = useMutation({ mutationFn: api.setAgentAccess, onSuccess: refresh });
  const setAccount = useMutation({
    mutationFn: ({ accountId, allowed }: { accountId: string; allowed: boolean }) =>
      api.setAgentAccountAccess(accountId, allowed),
    onSuccess: refresh,
  });
  const clearLog = useMutation({
    mutationFn: api.clearAgentLog,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["agent-log"] }),
  });

  const enabled = settings.data?.enabled ?? false;
  const accounts = settings.data?.accounts ?? [];
  const shared = accounts.filter((a) => a.allowed).length;
  const error = setEnabled.error ?? setAccount.error ?? clearLog.error;
  const command = launch.data
    ? ["claude", "mcp", "add", "elevant-mail", "--", launch.data.program, ...launch.data.args]
        .map(shellWord)
        .join(" ")
    : "";
  const config = launch.data
    ? JSON.stringify(
        {
          mcpServers: { "elevant-mail": { command: launch.data.program, args: launch.data.args } },
        },
        null,
        2,
      )
    : "";

  return (
    <div className="grid gap-8">
      <p className="text-sm text-muted-foreground">{t("agents.intro")}</p>

      <section className="grid gap-4">
        <div className="flex items-start justify-between gap-4">
          <div className="grid gap-0.5">
            <label htmlFor="agents-enabled" className="text-sm font-medium">
              {t("agents.enabled")}
            </label>
            <p id="agents-enabled-hint" className="text-sm text-muted-foreground">
              {t("agents.enabledHint")}
            </p>
          </div>
          <Switch
            id="agents-enabled"
            aria-describedby="agents-enabled-hint"
            checked={enabled}
            disabled={settings.isPending || setEnabled.isPending}
            onCheckedChange={(on) => setEnabled.mutate(on)}
            className="mt-0.5"
          />
        </div>

        {enabled && (
          <div className="grid gap-2">
            <h3 className="text-sm font-medium">{t("agents.accounts")}</h3>
            <ul className="divide-y rounded-lg border px-3">
              {accounts.map((account) => (
                <li key={account.accountId} className="flex items-center gap-3 py-2.5">
                  <span
                    aria-hidden
                    className="size-2.5 shrink-0 rounded-full"
                    style={{ backgroundColor: account.color }}
                  />
                  <label
                    htmlFor={`agents-account-${account.accountId}`}
                    className="grid min-w-0 flex-1"
                  >
                    <span className="truncate text-sm font-medium">{account.displayName}</span>
                    <span className="truncate text-xs text-muted-foreground">
                      {account.emailAddress}
                    </span>
                  </label>
                  <Switch
                    id={`agents-account-${account.accountId}`}
                    checked={account.allowed}
                    disabled={setAccount.isPending}
                    onCheckedChange={(allowed) =>
                      setAccount.mutate({ accountId: account.accountId, allowed })
                    }
                  />
                </li>
              ))}
            </ul>
            {shared === 0 && <Notice tone="info" text={t("agents.noneShared")} />}
          </div>
        )}
        {error && <Notice tone="error" text={describeError(error).message} />}
      </section>

      {enabled && (
        <section className="grid gap-3">
          <div className="grid gap-1">
            <h3 className="text-sm font-medium">{t("agents.connect")}</h3>
            <p className="text-sm text-muted-foreground">{t("agents.connectHint")}</p>
          </div>
          {command && <CopyBlock label={t("agents.claudeCode")} text={command} />}
          {config && <CopyBlock label={t("agents.otherClients")} text={config} />}
        </section>
      )}

      <section className="grid gap-2">
        <div className="flex items-center justify-between gap-2">
          <h3 className="text-sm font-medium">{t("agents.log")}</h3>
          {(log.data?.length ?? 0) > 0 && (
            <Button
              variant="ghost"
              size="sm"
              disabled={clearLog.isPending}
              onClick={() => clearLog.mutate()}
            >
              <Icon name="trash" />
              {t("agents.log.clear")}
            </Button>
          )}
        </div>
        {log.data && log.data.length > 0 ? (
          <ul className="divide-y rounded-lg border px-3">
            {log.data.map((entry, index) => (
              <LogEntry key={`${entry.at}-${index}`} entry={entry} />
            ))}
          </ul>
        ) : (
          <p className="text-sm text-muted-foreground">{t("agents.log.empty")}</p>
        )}
      </section>
    </div>
  );
}
