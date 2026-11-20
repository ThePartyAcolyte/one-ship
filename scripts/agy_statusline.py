#!/usr/bin/env python3
"""
ONE-SHIP Antigravity StatusLine Ingestion Hook
-----------------------------------------------
Receives JSON payload from Antigravity CLI on stdin and synchronously updates:
1. Account-level quota tracking (`account_quotas` table) by email or active account.
   Handles both Gemini pool ("gemini-5h", "gemini-weekly") and 3P/Anthropic pool
   ("3p-5h", "3p-weekly") emitted concurrently by Antigravity CLI.
2. Atomically writes local observation file at `~/.local/state/one-ship/agy-statusline.json`
   for immediate consumption by the ONE-SHIP daemon/Rust engine.
3. Conversation-level context & cache telemetry (`agent_session_metrics` table) by conversation_id.

Outputs a clean brutalist status indicator on stdout for terminal display.
"""

import json
import os
import re
import sqlite3
import sys
import time
from datetime import datetime
from pathlib import Path


def get_db_path() -> Path:
    env_path = os.environ.get("ONESHIP_DB_PATH")
    if env_path:
        return Path(env_path)
    config_dir = os.environ.get("XDG_CONFIG_HOME")
    if config_dir:
        base = Path(config_dir)
    else:
        base = Path.home() / ".config"
    return base / "one-ship" / "oneship.db"


def get_state_file_path() -> Path:
    env_path = os.environ.get("ONESHIP_STATUSLINE_FILE")
    if env_path:
        return Path(env_path)
    state_dir = os.environ.get("XDG_STATE_HOME")
    base = Path(state_dir) if state_dir else (Path.home() / ".local" / "state")
    return base / "one-ship" / "agy-statusline.json"


def get_log_file_path() -> Path:
    state_dir = os.environ.get("XDG_STATE_HOME")
    base = Path(state_dir) if state_dir else (Path.home() / ".local" / "state")
    return base / "one-ship" / "statusline.log"


def log_diagnostic(msg: str):
    try:
        log_path = get_log_file_path()
        log_path.parent.mkdir(parents=True, exist_ok=True)
        # Bounded ring log: if > 512KB, truncate keeping last 200 lines
        if log_path.exists() and log_path.stat().st_size > 512 * 1024:
            with open(log_path, "r", encoding="utf-8", errors="ignore") as f:
                lines = f.readlines()
            with open(log_path, "w", encoding="utf-8") as f:
                f.writelines(lines[-200:])
        with open(log_path, "a", encoding="utf-8") as f:
            f.write(f"[{time.strftime('%Y-%m-%d %H:%M:%S')}] {msg}\n")
    except Exception:
        pass


def canonical_pool(provider: str, model_name: str) -> tuple[str, str]:
    m = (model_name or "").lower()
    p = (provider or "").lower()
    if any(k in m for k in ("claude", "sonnet", "opus", "haiku", "gpt", "o1", "o3", "o4")) or any(k in p for k in ("anthropic", "openai")):
        return "Anthropic", "3p-pool"
    return "Agy", "gemini-pool"


def parse_reset_epoch(reset_in_seconds, reset_time_str, now_epoch: int) -> int | None:
    if isinstance(reset_in_seconds, (int, float)) and reset_in_seconds >= 0:
        return int(now_epoch + reset_in_seconds)
    if isinstance(reset_time_str, (int, float)):
        return int(reset_time_str)
    if not reset_time_str or not isinstance(reset_time_str, str):
        return None
    s = reset_time_str.strip().strip("()")
    if not s or s.lower() in ("n/a", "none"):
        return None

    # Try ISO RFC 3339
    try:
        dt = datetime.fromisoformat(s.replace("Z", "+00:00"))
        return int(dt.timestamp())
    except Exception:
        pass

    # Try countdown e.g. "3h 20m", "5d 14h", "45m", "30s"
    total_secs = 0
    matched = False
    for val, unit in re.findall(r"(\d+)\s*([dhms])", s.lower()):
        matched = True
        v = int(val)
        if unit == "d":
            total_secs += v * 86400
        elif unit == "h":
            total_secs += v * 3600
        elif unit == "m":
            total_secs += v * 60
        elif unit == "s":
            total_secs += v
    if matched:
        return int(now_epoch + total_secs)
    return None


def parse_bucket(bucket: dict, now: int) -> tuple[float | None, str | None, int | None]:
    if not isinstance(bucket, dict):
        return None, None, None

    rem = bucket.get("remaining_fraction")
    if rem is not None:
        pct = round(float(rem) * 100.0, 2)
    else:
        rem_pct = bucket.get("remaining_percent")
        pct = round(float(rem_pct), 2) if rem_pct is not None else None

    sec = bucket.get("reset_in_seconds")
    resets_str = bucket.get("reset_time") or bucket.get("resets_at")
    reset_epoch = None
    if isinstance(sec, (int, float)) and sec >= 0:
        reset_epoch = int(now + sec)
    elif resets_str:
        reset_epoch = parse_reset_epoch(None, resets_str, now)

    countdown_str = None
    if reset_epoch is not None:
        diff = max(0, reset_epoch - now)
        if diff >= 86400:
            countdown_str = f"{diff // 86400}d {(diff % 86400) // 3600}h"
        elif diff >= 3600:
            countdown_str = f"{diff // 3600}h {(diff % 3600) // 60}m"
        else:
            countdown_str = f"{diff // 60}m"
    elif resets_str:
        countdown_str = str(resets_str)

    return pct, countdown_str, reset_epoch


def upsert_account_quota_row(
    conn: sqlite3.Connection,
    account_id: str,
    provider: str,
    model: str,
    five_h_pct: float | None,
    five_h_resets_at: str | None,
    five_h_epoch: int | None,
    weekly_pct: float | None,
    weekly_resets_at: str | None,
    weekly_epoch: int | None,
    now: int,
):
    conn.execute(
        """
        INSERT INTO account_quotas (
            account_id, provider, model,
            five_hour_remaining_pct, five_hour_resets_at, five_hour_reset_epoch_seconds,
            weekly_remaining_pct, weekly_resets_at, weekly_reset_epoch_seconds,
            updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(account_id, provider, model) DO UPDATE SET
            five_hour_remaining_pct = coalesce(excluded.five_hour_remaining_pct, account_quotas.five_hour_remaining_pct),
            five_hour_resets_at = coalesce(excluded.five_hour_resets_at, account_quotas.five_hour_resets_at),
            five_hour_reset_epoch_seconds = coalesce(excluded.five_hour_reset_epoch_seconds, account_quotas.five_hour_reset_epoch_seconds),
            weekly_remaining_pct = coalesce(excluded.weekly_remaining_pct, account_quotas.weekly_remaining_pct),
            weekly_resets_at = coalesce(excluded.weekly_resets_at, account_quotas.weekly_resets_at),
            weekly_reset_epoch_seconds = coalesce(excluded.weekly_reset_epoch_seconds, account_quotas.weekly_reset_epoch_seconds),
            updated_at = excluded.updated_at
        """,
        (
            account_id,
            provider,
            model,
            five_h_pct,
            five_h_resets_at,
            five_h_epoch,
            weekly_pct,
            weekly_resets_at,
            weekly_epoch,
            now,
        ),
    )


def process_payload(payload: dict):
    now = int(time.time())
    db_path = get_db_path()
    if not db_path.exists():
        log_diagnostic(f"Database file not found at {db_path}")
        return

    try:
        conn = sqlite3.connect(str(db_path), timeout=5.0)
        conn.execute("PRAGMA journal_mode = WAL;")
        conn.execute("PRAGMA synchronous = NORMAL;")
    except Exception as e:
        log_diagnostic(f"Failed to connect to SQLite: {e}")
        return

    try:
        # ─────────────────────────────────────────────────────────────
        # 1. Account-level Quota Tracking (by Email or Active Account)
        # ─────────────────────────────────────────────────────────────
        email = (
            payload.get("email")
            or payload.get("user_email")
            or payload.get("user")
            or ""
        ).strip().lower()

        raw_model = ""
        model_obj = payload.get("model")
        if isinstance(model_obj, dict):
            raw_model = model_obj.get("id") or model_obj.get("display_name") or ""
        elif isinstance(model_obj, str):
            raw_model = model_obj

        provider = payload.get("provider") or "Agy"
        canon_prov, canon_model = canonical_pool(provider, raw_model)

        account_id = None
        if email:
            cur = conn.cursor()
            cur.execute("SELECT id FROM accounts WHERE lower(email) = ? LIMIT 1", (email,))
            row = cur.fetchone()
            if row:
                account_id = row[0]
        else:
            # Fallback to active account only if email was omitted from payload
            cur = conn.cursor()
            cur.execute("SELECT id FROM accounts WHERE is_active = 1 LIMIT 1")
            row = cur.fetchone()
            if row:
                account_id = row[0]

        quota = payload.get("quota") or {}
        if account_id and isinstance(quota, dict):
            updated_any = False

            # Case A: Named pool dictionaries (Real Antigravity format)
            gemini_5h_bucket = quota.get("gemini-5h")
            gemini_weekly_bucket = quota.get("gemini-weekly")
            p3_5h_bucket = quota.get("3p-5h") or quota.get("claude-5h")
            p3_weekly_bucket = quota.get("3p-weekly") or quota.get("claude-weekly")

            if gemini_5h_bucket or gemini_weekly_bucket:
                g_5h_pct, g_5h_res, g_5h_ep = parse_bucket(gemini_5h_bucket, now)
                g_w_pct, g_w_res, g_w_ep = parse_bucket(gemini_weekly_bucket, now)
                if g_5h_pct is not None or g_w_pct is not None:
                    upsert_account_quota_row(
                        conn, account_id, "Agy", "gemini-pool",
                        g_5h_pct, g_5h_res, g_5h_ep,
                        g_w_pct, g_w_res, g_w_ep, now
                    )
                    updated_any = True

            if p3_5h_bucket or p3_weekly_bucket:
                p_5h_pct, p_5h_res, p_5h_ep = parse_bucket(p3_5h_bucket, now)
                p_w_pct, p_w_res, p_w_ep = parse_bucket(p3_weekly_bucket, now)
                if p_5h_pct is not None or p_w_pct is not None:
                    upsert_account_quota_row(
                        conn, account_id, "Anthropic", "3p-pool",
                        p_5h_pct, p_5h_res, p_5h_ep,
                        p_w_pct, p_w_res, p_w_ep, now
                    )
                    updated_any = True

            # Case B: Windows array fallback (Herder or synthetic fixtures)
            if not updated_any:
                windows = quota.get("windows") or payload.get("windows") or []
                five_h_pct = None
                five_h_resets_at = None
                five_h_reset_epoch = None
                weekly_pct = None
                weekly_resets_at = None
                weekly_reset_epoch = None

                for w in windows:
                    kind = (w.get("kind") or w.get("label") or "").lower()
                    rem = w.get("remaining_fraction")
                    if rem is not None:
                        pct = round(float(rem) * 100.0, 2)
                    else:
                        rem_pct = w.get("remaining_percent")
                        pct = round(float(rem_pct), 2) if rem_pct is not None else None

                    resets_str = w.get("reset_time") or w.get("resets_at")
                    reset_sec = w.get("reset_in_seconds")
                    reset_ep = parse_reset_epoch(reset_sec, resets_str, now) or w.get("reset_epoch")

                    countdown_str = None
                    if reset_ep is not None:
                        diff = max(0, reset_ep - now)
                        if diff >= 86400:
                            countdown_str = f"{diff // 86400}d {(diff % 86400) // 3600}h"
                        elif diff >= 3600:
                            countdown_str = f"{diff // 3600}h {(diff % 3600) // 60}m"
                        else:
                            countdown_str = f"{diff // 60}m"
                    elif resets_str:
                        countdown_str = str(resets_str)

                    if "5h" in kind or "hour" in kind:
                        five_h_pct = pct
                        five_h_resets_at = countdown_str
                        five_h_reset_epoch = reset_ep
                    elif "week" in kind or "7d" in kind:
                        weekly_pct = pct
                        weekly_resets_at = countdown_str
                        weekly_reset_epoch = reset_ep

                # Direct fields fallback
                if five_h_pct is None and "five_hour_remaining_pct" in quota:
                    five_h_pct = quota.get("five_hour_remaining_pct")
                    five_h_resets_at = quota.get("five_hour_resets_at")
                    five_h_reset_epoch = quota.get("five_hour_reset_epoch")

                if weekly_pct is None and "weekly_remaining_pct" in quota:
                    weekly_pct = quota.get("weekly_remaining_pct")
                    weekly_resets_at = quota.get("weekly_resets_at")
                    weekly_reset_epoch = quota.get("weekly_reset_epoch")

                if five_h_pct is not None or weekly_pct is not None:
                    upsert_account_quota_row(
                        conn, account_id, canon_prov, canon_model,
                        five_h_pct, five_h_resets_at, five_h_reset_epoch,
                        weekly_pct, weekly_resets_at, weekly_reset_epoch, now
                    )
                    updated_any = True

        # ─────────────────────────────────────────────────────────────
        # 2. Local State Snapshot File for Rust Engine & Daemon
        # ─────────────────────────────────────────────────────────────
        try:
            obs_path = get_state_file_path()
            obs_path.parent.mkdir(parents=True, exist_ok=True)
            tmp_path = obs_path.with_suffix(".json.tmp")
            with open(tmp_path, "w", encoding="utf-8") as f:
                json.dump({
                    "timestamp": now,
                    "account_id": account_id,
                    "email": email,
                    "provider": canon_prov,
                    "model": raw_model,
                    "quota": quota,
                    "context_window": payload.get("context_window") or {},
                    "conversation_id": payload.get("conversation_id") or "",
                    "raw_payload": payload,
                }, f, indent=2)
            tmp_path.replace(obs_path)
        except Exception as e:
            log_diagnostic(f"Failed to write state file {obs_path}: {e}")

        # ─────────────────────────────────────────────────────────────
        # 3. Conversation-level Context & Telemetry (by conversation_id)
        # ─────────────────────────────────────────────────────────────
        conv_id = (
            payload.get("conversation_id")
            or payload.get("session_id")
            or ""
        ).strip()

        context_window = payload.get("context_window") or {}
        if conv_id and context_window:
            cur = conn.cursor()
            cur.execute(
                """
                SELECT conversation_id, project_id, agent_id
                FROM agent_chat_turns
                WHERE agy_conversation_id = ?
                ORDER BY timestamp DESC
                LIMIT 1
                """,
                (conv_id,),
            )
            conv_row = cur.fetchone()

            if conv_row:
                internal_conv_id, project_id, agent_id = conv_row

                used_pct = context_window.get("used_percent") or context_window.get("used_pct") or 0.0
                fresh = context_window.get("fresh_tokens") or context_window.get("input_tokens") or 0
                cache_read = context_window.get("cache_read_tokens") or context_window.get("cache_read_input_tokens") or 0
                cache_create = context_window.get("cache_creation_tokens") or 0
                total_tokens = context_window.get("total_tokens") or (fresh + cache_read + cache_create)
                cache_hit = context_window.get("cache_hit_pct") or 0.0
                if total_tokens > 0 and cache_hit == 0.0 and cache_read > 0:
                    cache_hit = round((cache_read / total_tokens) * 100.0, 1)

                conn.execute(
                    """
                    INSERT INTO agent_session_metrics (
                        agy_conversation_id, conversation_id, project_id, agent_id,
                        provider, model, context_used_pct, fresh_tokens_count, cache_read_tokens_count,
                        cache_creation_tokens_count, total_tokens_count, cache_hit_pct, updated_at
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(agy_conversation_id) DO UPDATE SET
                        context_used_pct = excluded.context_used_pct,
                        fresh_tokens_count = excluded.fresh_tokens_count,
                        cache_read_tokens_count = excluded.cache_read_tokens_count,
                        cache_creation_tokens_count = excluded.cache_creation_tokens_count,
                        total_tokens_count = excluded.total_tokens_count,
                        cache_hit_pct = excluded.cache_hit_pct,
                        model = coalesce(excluded.model, agent_session_metrics.model),
                        updated_at = excluded.updated_at
                    """,
                    (
                        conv_id,
                        internal_conv_id,
                        project_id,
                        agent_id,
                        canon_prov,
                        raw_model or canon_model,
                        used_pct,
                        fresh,
                        cache_read,
                        cache_create,
                        total_tokens,
                        cache_hit,
                        now,
                    ),
                )

        conn.commit()
    except Exception as e:
        log_diagnostic(f"Error executing database update: {e}")
    finally:
        try:
            conn.close()
        except Exception:
            pass


def format_statusline(payload: dict) -> str:
    raw_model = ""
    model_obj = payload.get("model")
    if isinstance(model_obj, dict):
        raw_model = model_obj.get("display_name") or model_obj.get("id") or ""
    elif isinstance(model_obj, str):
        raw_model = model_obj

    quota = payload.get("quota") or {}
    now = int(time.time())

    five_h_str = ""
    week_str = ""

    # Check named buckets first
    b_5h = quota.get("gemini-5h") or quota.get("3p-5h")
    b_w = quota.get("gemini-weekly") or quota.get("3p-weekly")
    if isinstance(b_5h, dict):
        pct, cd, _ = parse_bucket(b_5h, now)
        if pct is not None:
            five_h_str = f"5h: {pct:.0f}%"
            if cd:
                five_h_str += f" ({cd})"
    if isinstance(b_w, dict):
        pct, cd, _ = parse_bucket(b_w, now)
        if pct is not None:
            week_str = f"7d: {pct:.0f}%"
            if cd:
                week_str += f" ({cd})"

    # Fallback to windows list
    if not five_h_str:
        windows = quota.get("windows") or payload.get("windows") or []
        for w in windows:
            kind = (w.get("kind") or w.get("label") or "").lower()
            if "5h" in kind or "hour" in kind:
                rem = w.get("remaining_fraction")
                pct = round(rem * 100) if rem is not None else w.get("remaining_percent")
                resets = w.get("reset_time") or ""
                if pct is not None:
                    five_h_str = f"5h: {pct}%"
                    if resets:
                        five_h_str += f" ({resets})"
                break

    context_window = payload.get("context_window") or {}
    ctx_pct = context_window.get("used_percent") or context_window.get("used_pct")
    ctx_str = f"ctx: {int(ctx_pct)}%" if ctx_pct is not None else ""

    parts = ["[ONE-SHIP]"]
    if raw_model:
        parts.append(raw_model)
    if five_h_str:
        parts.append(five_h_str)
    if week_str:
        parts.append(week_str)
    if ctx_str:
        parts.append(ctx_str)

    return " | ".join(parts)


def main():
    try:
        raw = sys.stdin.read()
        if not raw.strip():
            print("[ONE-SHIP]")
            return
        payload = json.loads(raw)
    except Exception as e:
        log_diagnostic(f"Failed to read/parse stdin JSON: {e}")
        print("[ONE-SHIP]")
        return

    process_payload(payload)
    print(format_statusline(payload))


if __name__ == "__main__":
    main()
