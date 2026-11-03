#!/usr/bin/env python3
"""Seed a test stand from private JSON on stdin; never print passwords or TOTP.

Requires python3-argon2 and python3-cryptography. Run as root after migrations.
The payload stays on the deployment workstation; PostgreSQL stores only hashes.
"""
import hashlib
import json
import secrets
import subprocess
import sys
import uuid
from pathlib import Path

from argon2 import PasswordHasher
from cryptography.hazmat.primitives.ciphers.aead import AESGCM


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


def query(sql):
    result = subprocess.run(
        ['runuser', '-u', 'dropit', '--', 'psql', '-X', '-qAt', '-d', 'dropit', '-v', 'ON_ERROR_STOP=1'],
        input=sql, text=True, capture_output=True,
    )
    if result.returncode:
        raise RuntimeError('Test stand bootstrap SQL failed; no credentials were printed')
    return result.stdout.strip()


def main():
    data = json.load(sys.stdin)
    env = {}
    for line in Path('/srv/dropit/etc/dropit.env').read_text().splitlines():
        if line and not line.startswith('#') and '=' in line:
            key, value = line.split('=', 1)
            env[key] = value.strip().strip("'\"")
    if env.get('STRIPE_API_KEY') or env.get('STRIPE_WEBHOOK_SECRET'):
        raise RuntimeError('This bootstrap is for a mock-payment test stand only')
    hasher = PasswordHasher(time_cost=2, memory_cost=19456, parallelism=1)
    club = data['club']
    dj = data['dj']
    admin = data['admin']
    club_id = query(f"SELECT id FROM club WHERE email={literal(club['email'])};") or str(uuid.uuid4())
    dj_id = query(f"SELECT id FROM dj WHERE email={literal(dj['email'])};") or str(uuid.uuid4())
    admin_id = query(f"SELECT id FROM admin_user WHERE email={literal(admin['email'])};") or str(uuid.uuid4())
    nonce = secrets.token_bytes(12)
    ciphertext = AESGCM(bytes.fromhex(env['DROPIT_TOTP_KEY'])).encrypt(nonce, admin['totp_secret'].encode(), None)
    encrypted_totp = f'v2:{nonce.hex()}:{ciphertext.hex()}'.encode().hex()
    # Existing accounts are preserved on a repeated bootstrap.
    query(f"""
BEGIN;
INSERT INTO club (id,name,slug,email,password_hash,status,address,country)
VALUES ({literal(club_id)},{literal(club['name'])},{literal(club['slug'])},{literal(club['email'])},
        {literal(hasher.hash(club['password']))},'active','Test stand','RU') ON CONFLICT (email) DO NOTHING;
INSERT INTO dj (id,club_id,display_name,email,password_hash,status)
VALUES ({literal(dj_id)},{literal(club_id)},{literal(dj['name'])},{literal(dj['email'])},
        {literal(hasher.hash(dj['password']))},'active') ON CONFLICT (email) DO NOTHING;
INSERT INTO admin_user (id,email,password_hash,totp_secret_enc,totp_confirmed_at,role,status,display_name)
VALUES ({literal(admin_id)},{literal(admin['email'])},{literal(hasher.hash(admin['password']))},
        decode({literal(encrypted_totp)},'hex'),now(),'superadmin','active','Test stand owner')
ON CONFLICT (email) DO NOTHING;
UPDATE platform_settings SET commission_mode='percent',commission_percent_bp=3000,
  min_price_minor=100,max_price_minor=10000,dj_share_of_remainder_bp=5000,currency='EUR',runtime_mode='test'
WHERE id=1;
COMMIT;
""")
    pricing_id = query(f"SELECT id FROM pricing_policy WHERE club_id={literal(club_id)} AND valid_to IS NULL ORDER BY valid_from DESC LIMIT 1;") or str(uuid.uuid4())
    session_id = query(f"SELECT id FROM session WHERE club_id={literal(club_id)} AND status<>'closed' ORDER BY created_at DESC LIMIT 1;") or str(uuid.uuid4())
    token_hash = hashlib.sha256(data['qr_token'].encode()).hexdigest()
    # A two-week QR is deliberate for this test room, so the handoff remains
    # usable after tonight. Normal sessions still use the regular 07:00 rule.
    query(f"""
BEGIN;
INSERT INTO pricing_policy (id,club_id,price_minor,created_by)
VALUES ({literal(pricing_id)},{literal(club_id)},300,{literal(club_id)}) ON CONFLICT (id) DO NOTHING;
INSERT INTO session (id,club_id,dj_id,status,pricing_policy_id,opened_at,timezone)
VALUES ({literal(session_id)},{literal(club_id)},{literal(dj_id)},'active',{literal(pricing_id)},now(),'Europe/Moscow')
ON CONFLICT (id) DO NOTHING;
UPDATE qr_token SET active=false WHERE session_id={literal(session_id)} AND token_hash<>{literal(token_hash)};
INSERT INTO qr_token (id,session_id,token_hash,active,expires_at)
VALUES ({literal(uuid.uuid4())},{literal(session_id)},{literal(token_hash)},true,now()+interval '14 days')
ON CONFLICT (token_hash) DO UPDATE SET active=true,expires_at=EXCLUDED.expires_at;
INSERT INTO audit_log (actor_type,actor_id,action,entity,diff)
VALUES ('system',{literal(admin_id)},'test_stand_bootstrap',{literal(club_id)},'{{"test_mode":true}}');
COMMIT;
""")
    print(json.dumps({'club_id': club_id, 'dj_id': dj_id, 'admin_id': admin_id,
                      'session_id': session_id, 'test_payments': True, 'price_minor': 300}))


if __name__ == '__main__':
    main()
