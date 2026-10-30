import { useState } from "react";

import { Icon } from "@/components/common/Icon";
import { Button } from "@/components/ui/button";
import { describeError } from "@/lib/errors";
import { t, tCount } from "@/lib/i18n";
import { api } from "@/lib/tauri";
import { useAccounts } from "@/queries/accounts";
import { useSearchQuery } from "@/queries/messages";
import { useUiStore } from "@/stores/ui";

/**
 * Below the search results: searches all accounts on their servers for what the cache does not
 * have (older mail, words deep in large mail). Only on request; what it finds joins the results.
 */
export function SearchOnServer() {
  const query = useSearchQuery();
  const { data: accounts = [] } = useAccounts();
  const hits = useUiStore((s) => (s.serverHits?.query === query ? s.serverHits : null));
  const setServerHits = useUiStore((s) => s.setServerHits);
  const [running, setRunning] = useState<string | null>(null);

  const search = async () => {
    const asked = query;
    setRunning(asked);
    const results = await Promise.allSettled(
      accounts.map((account) => api.searchServer(account.id, asked)),
    );
    setServerHits({
      query: asked,
      ids: results.flatMap((result) => (result.status === "fulfilled" ? result.value : [])),
      failed: results.flatMap((result, index) => {
        const account = accounts[index];
        if (result.status === "fulfilled" || !account) return [];
        return [{ accountId: account.id, message: describeError(result.reason).message }];
      }),
    });
    setRunning(null);
  };

  if (query === "") return null;
  if (running === query) {
    return (
      <p
        role="status"
        className="flex items-center justify-center gap-2 px-4 py-3 text-xs text-muted-foreground"
      >
        <Icon name="spinner" className="size-3.5 animate-spin" />
        {t("search.server.running")}
      </p>
    );
  }
  if (hits) {
    return (
      <div role="status" className="grid gap-1 px-4 py-3 text-center text-xs text-muted-foreground">
        <p>
          {hits.ids.length > 0
            ? tCount("search.server.found", hits.ids.length)
            : t("search.server.nothing")}
        </p>
        {hits.failed.map((failure) => (
          <p key={failure.accountId} className="text-destructive">
            {accounts.find((a) => a.id === failure.accountId)?.displayName}: {failure.message}
          </p>
        ))}
      </div>
    );
  }
  return (
    <div className="flex justify-center px-2 py-2">
      <Button
        variant="ghost"
        size="sm"
        disabled={accounts.length === 0}
        onClick={() => void search()}
      >
        <Icon name="server" />
        {t("search.server")}
      </Button>
    </div>
  );
}
