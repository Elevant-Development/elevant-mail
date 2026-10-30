import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { api } from "@/lib/tauri";

/** Senders whose mail always shows its external images. */
export function useImageSenders() {
  return useQuery({ queryKey: ["imageSenders"], queryFn: api.listImageSenders });
}

/** Adds or removes a sender; open mail from it shows or hides its images at once. */
export function useSetImageSender() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ address, allowed }: { address: string; allowed: boolean }) =>
      allowed ? api.allowImagesFrom(address) : api.forbidImagesFrom(address),
    onSettled: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: ["imageSenders"] }),
        queryClient.invalidateQueries({ queryKey: ["message"] }),
      ]),
  });
}
