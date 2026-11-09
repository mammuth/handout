export type Kind = 'html' | 'markdown' | 'pdf' | 'image' | 'other';
export type ViewStateKind = 'ok' | 'password' | 'email' | 'expired' | 'forbidden' | 'off';

export interface SidebarEntry {
  id: string;
  title: string;
  kind: Kind;
  updated_at: string;
  is_new: boolean;
  link: string;
}

export interface ViewResponse {
  owner: { slug: string; name: string };
  recipient: { name: string; email: string | null } | null;
  file: { id: string; title: string; kind: Kind; version: number; filename: string; size: number; mime: string; updated_at: string };
  state: ViewStateKind;
  /** The signed-in owner of this file: gates skipped, nothing recorded. */
  is_owner: boolean;
  content_url: string | null;
  download_url: string | null;
  open: { id: number; token: string } | null;
  sidebar: SidebarEntry[];
}

export interface HomeResponse {
  owner: { slug: string; name: string };
  recipient: { name: string; email: string | null };
  sidebar: SidebarEntry[];
}

export interface ApiError {
  status: number;
  code: string;
  message: string;
}
