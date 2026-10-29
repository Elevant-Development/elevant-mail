import { splitUrl } from "@/lib/links";

/** An address with its host emphasized: that part decides where a link really leads. */
export function LinkAddress({ url }: { url: string }) {
  const { before, host, after } = splitUrl(url);
  return (
    <span>
      <span className="text-muted-foreground">{before}</span>
      <span className="font-semibold text-foreground">{host}</span>
      <span className="text-muted-foreground">{after}</span>
    </span>
  );
}
