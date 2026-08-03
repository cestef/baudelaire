/** Request path to fingerprinted URL, for every asset this build renamed. */
const assets: Record<string, string>;
/** The served URL for a request path, or the path itself when it was not renamed. */
export function url(path: string): string;
export default assets;
