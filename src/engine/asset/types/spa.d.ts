export interface RouterOptions {
  /** Fetches a route's document. `mountSpa` supplies the built site's. */
  load: (path: string) => Promise<string>;
  /** Whether a path is one this site serves. Every path, when absent. */
  known?: ((path: string) => boolean) | null;
  /** The container swapped on navigation. The document body, when absent. */
  select?: string | null;
  mode?: "history" | "hash";
  prefetch?: Prefetch;
  /** The route the first render is already showing. */
  entry?: string | null;
}

/** The mounted router, or the one already driving this document. */
export function mountSpa(options?: Partial<RouterOptions>): unknown;
export function mountRouter(options: RouterOptions): unknown;
