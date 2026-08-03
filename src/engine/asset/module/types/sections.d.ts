export interface Section {
  id: string;
  pages: Array<{ url: string; title: string }>;
  children: Section[];
}

/** The section trees, keyed by language code. */
const sections: Record<string, Section[]>;
export default sections;
