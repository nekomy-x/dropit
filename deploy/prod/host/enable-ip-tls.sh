#!/usr/bin/env bash
# HTTPS for an IP-only test host. Certbot >=5.4 supports short-lived IP certs.
# Ref: https://letsencrypt.org/2026/03/11/shorter-certs-certbot.html
set -euo pipefail
test "$(id -u)" = 0
DROPIT_TLS_IP=${1:?public IP required}
python3 -c 'import ipaddress,sys; ipaddress.ip_address(sys.argv[1])' "$DROPIT_TLS_IP"
export DEBIAN_FRONTEND=noninteractive
apt-get install -y -qq python3-venv
test -x /opt/dropit-certbot/bin/certbot || {
  python3 -m venv /opt/dropit-certbot
  /opt/dropit-certbot/bin/pip install --quiet 'certbot>=5.4,<6'
}
install -d -m 755 /var/lib/dropit-acme/.well-known/acme-challenge
/opt/dropit-certbot/bin/certbot certonly --non-interactive --agree-tos \
  --register-unsafely-without-email --preferred-profile shortlived \
  --webroot --webroot-path /var/lib/dropit-acme --ip-address "$DROPIT_TLS_IP" \
  --cert-name dropit-ip --deploy-hook 'nginx -t && systemctl reload nginx'
cat > /srv/dropit/etc/nginx-listeners-ip.conf <<'EOF'
listen 443 ssl;
listen [::]:443 ssl;
ssl_certificate /etc/letsencrypt/live/dropit-ip/fullchain.pem;
ssl_certificate_key /etc/letsencrypt/live/dropit-ip/privkey.pem;
ssl_protocols TLSv1.2 TLSv1.3;
ssl_session_cache shared:DROPIT_SSL:1m;
ssl_session_timeout 1d;
EOF
chmod 600 /srv/dropit/etc/nginx-listeners-ip.conf
nginx -t
systemctl reload nginx
cat >/etc/systemd/system/dropit-certbot-renew.service <<'EOF'
[Unit]
Description=Renew DROP IT IP certificate
After=network-online.target nginx.service
Wants=network-online.target
[Service]
Type=oneshot
ExecStart=/opt/dropit-certbot/bin/certbot renew --quiet --no-random-sleep-on-renew
TimeoutStartSec=300
EOF
cat >/etc/systemd/system/dropit-certbot-renew.timer <<'EOF'
[Unit]
Description=Check DROP IT certificate renewal twice daily
[Timer]
OnCalendar=*-*-* 00,12:00:00
RandomizedDelaySec=3600
Persistent=true
[Install]
WantedBy=timers.target
EOF
systemctl daemon-reload
systemctl enable --now dropit-certbot-renew.timer
curl --fail --silent --show-error --output /dev/null "https://$DROPIT_TLS_IP/"
printf 'HTTPS ready: https://%s/\n' "$DROPIT_TLS_IP"
