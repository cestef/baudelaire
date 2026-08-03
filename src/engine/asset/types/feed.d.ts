/** One recent dated page, newest first. */
export interface Recent {
  url: string;
  title: string;
  lang: string;
  /** ISO 8601. */
  date: string | null;
}

const feed: Recent[];
export default feed;
