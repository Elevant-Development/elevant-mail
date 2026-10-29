import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api, type NewAccount } from "@/lib/tauri";

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
