import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { type AccountChanges, api, type ImapSettings, type NewAccount } from "@/lib/tauri";
import { useSyncStore } from "@/stores/sync";

export function useAccounts() {
  return useQuery({ queryKey: ["accounts"], queryFn: api.listAccounts });
}

/** Adding or removing an account changes folders and messages too. */
function useRefreshMailData() {
  const queryClient = useQueryClient();
  return () =>
    Promise.all(
      ["accounts", "folders", "messages"].map((key) =>
        queryClient.invalidateQueries({ queryKey: [key] }),
      ),
    );
}

export function useAddAccount() {
  const refresh = useRefreshMailData();
  return useMutation({
    /** `password: null` signs in with Microsoft in the browser instead. */
    mutationFn: ({ account, password }: { account: NewAccount; password: string | null }) =>
      password === null ? api.addMicrosoftAccount(account) : api.addAccount(account, password),
    onSuccess: refresh,
  });
}

export function useRemoveAccount() {
  const refresh = useRefreshMailData();
  return useMutation({
    mutationFn: (accountId: string) => api.removeAccount(accountId),
    onSuccess: refresh,
  });
}

export function useAccountSettings(accountId: string) {
  return useQuery({
    queryKey: ["account-settings", accountId],
    queryFn: () => api.getAccountSettings(accountId),
  });
}

/** How far back the account keeps mail; `null` keeps everything. */
export function useSetKeepDays() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ accountId, days }: { accountId: string; days: number | null }) =>
      api.setKeepDays(accountId, days),
    onSuccess: (_, { accountId }) =>
      queryClient.invalidateQueries({ queryKey: ["account-settings", accountId] }),
  });
}

export function useUpdateAccount() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ accountId, changes }: { accountId: string; changes: AccountChanges }) =>
      api.updateAccount(accountId, changes),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["accounts"] }),
  });
}

/** After a new login the account's old sync error no longer applies. */
function useLoginChanged() {
  const queryClient = useQueryClient();
  const clearError = useSyncStore((s) => s.clearError);
  return (accountId: string) => {
    clearError(accountId);
    return Promise.all(
      [["account-settings", accountId], ["folders"]].map((queryKey) =>
        queryClient.invalidateQueries({ queryKey }),
      ),
    );
  };
}

export function useChangeLogin() {
  const changed = useLoginChanged();
  return useMutation({
    /** `password: null` keeps the stored one. */
    mutationFn: ({
      accountId,
      imap,
      password,
    }: {
      accountId: string;
      imap: ImapSettings;
      password: string | null;
    }) => api.changeLogin(accountId, imap, password),
    onSuccess: (_, { accountId }) => changed(accountId),
  });
}

export function useSignInAgain() {
  const changed = useLoginChanged();
  return useMutation({
    mutationKey: ["sign-in-again"],
    mutationFn: (accountId: string) => api.signInAgain(accountId),
    onSuccess: (_, accountId) => changed(accountId),
  });
}
