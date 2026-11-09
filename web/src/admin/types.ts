import type { Kind } from '../lib/types';

export type Access = 'link' | 'recipients' | 'off';

export interface Owner { slug: string; name: string; public_url: string; mcp_url: string }

export interface FileSummary {
  id: string; title: string; kind: Kind; access: Access; has_password: boolean; email_gate: boolean;
  expires_at: string | null; version: number; filename: string; size: number; mime: string;
  created_at: string; updated_at: string; file_link: string;
}
export interface RecipientJson { id: string; name: string; email: string | null; company: string | null; created_at: string; last_seen_at: string | null }
export interface InviteJson { recipient: RecipientJson; created_at: string; last_opened_at: string | null; recipient_link: string; opens: number; active_seconds: number; max_scroll_pct: number }
export interface VersionJson { number: number; size: number; mime: string; filename: string; created_at: string }
export interface FileDetail extends FileSummary { versions: VersionJson[]; invites: InviteJson[]; opens: number; visible_seconds: number; active_seconds: number }
export interface RecipientFile extends FileSummary { invited_at: string; last_opened_at: string | null; recipient_link: string; opens: number; active_seconds: number; max_scroll_pct: number }
export interface RecipientDetail extends RecipientJson { token: string; files: RecipientFile[] }
export interface OpenJson { file_id: string; title: string; version: number; recipient: { id: string; name: string } | null; at: string; user_agent: string; visible_seconds: number; active_seconds: number; max_scroll_pct: number }
export interface ShareResult { file_id: string; recipient: RecipientJson; recipient_link: string; invite_created: boolean; recipient_created: boolean }
export interface ApiKeyJson { id: number; name: string; prefix: string; created_at: string; last_used_at: string | null; revoked_at: string | null; key?: string }
