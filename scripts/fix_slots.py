#!/usr/bin/env python3
"""Backfill slot=0 event rows by fetching the real slot from an RPC node.

Usage:
    ./fix_slots.py --db sqlite:./soltrace.db --rpc-url https://api.mainnet-beta.solana.com
    ./fix_slots.py --db postgres://user:pass@host/db --rpc-url https://...

The events table stores (signature, slot, ...). Rows where slot=0 are the
result of indexing without slot context; this script looks up each distinct
signature via getTransaction and writes the real slot back. Idempotent:
re-running only touches rows still at slot=0.
"""

import argparse
import json
import sqlite3
import sys
import time
from urllib import request as urlreq
from urllib.error import HTTPError, URLError


def connect(db_url: str):
    if db_url.startswith("sqlite:"):
        conn = sqlite3.connect(db_url[len("sqlite:") :])
        conn.row_factory = sqlite3.Row
        return conn
    if db_url.startswith("postgres"):
        import psycopg2  # only needed if you actually use postgres
        import psycopg2.extras

        conn = psycopg2.connect(db_url)
        conn.set_session(autocommit=False)
        return conn
    raise SystemExit(f"unsupported db url scheme: {db_url}")


def rpc_call(rpc_url: str, method: str, params: list, timeout: int = 30) -> dict:
    payload = json.dumps(
        {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    ).encode()
    req = urlreq.Request(
        rpc_url, data=payload, headers={"Content-Type": "application/json"}
    )
    with urlreq.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read())


def fetch_slot(rpc_url: str, signature: str, max_retries: int = 4) -> int | None:
    params = [signature, {"maxSupportedTransactionVersion": 1}]
    backoff = 1.0
    for _ in range(max_retries):
        try:
            res = rpc_call(rpc_url, "getTransaction", params)
        except (HTTPError, URLError, TimeoutError) as e:
            print(
                f"  net err {signature[:16]}..: {e}; retry in {backoff}s",
                file=sys.stderr,
            )
            time.sleep(backoff)
            backoff *= 2
            continue
        if "error" in res:
            print(f"  rpc err {signature[:16]}..: {res['error']}", file=sys.stderr)
            return None
        tx = res.get("result")
        return tx["slot"] if tx else None
    print(
        f"  giving up on {signature[:16]}.. after {max_retries} retries",
        file=sys.stderr,
    )
    return None


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--db", required=True, help="sqlite:./path.db or postgres://...")
    ap.add_argument("--rpc-url", default="https://api.mainnet-beta.solana.com")
    ap.add_argument(
        "--delay",
        type=float,
        default=0.2,
        help="seconds between RPC calls (rate-limit backoff)",
    )
    args = ap.parse_args()

    conn = connect(args.db)
    is_pg = args.db.startswith("postgres")
    placeholder = "%s" if is_pg else "?"

    cur = conn.cursor()
    cur.execute(f"SELECT DISTINCT signature FROM events WHERE slot = 0")
    sigs = [r[0] for r in cur.fetchall()]
    print(f"found {len(sigs)} distinct signatures with slot=0")

    done = skipped = 0
    for i, sig in enumerate(sigs, 1):
        print(f"Taking on {sig}")
        slot = fetch_slot(args.rpc_url, sig)
        if slot is None:
            skipped += 1
            continue
        cur.execute(
            f"UPDATE events SET slot = {placeholder} WHERE signature = {placeholder} AND slot = 0",
            (slot, sig),
        )
        conn.commit()
        done += 1
        print(f"[{i}/{len(sigs)}] {sig[:16]}.. -> slot {slot} ({cur.rowcount} rows)")
        if i < len(sigs):
            time.sleep(args.delay)

    conn.close()
    print(f"\nupdated {done} signatures, skipped {skipped}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
