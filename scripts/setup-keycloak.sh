#!/usr/bin/env bash
set -euo pipefail

KC_URL="${KC_URL:-http://localhost:8081}"
KC_ADMIN="${KEYCLOAK_ADMIN:-admin}"
KC_PASS="${KEYCLOAK_ADMIN_PASSWORD:-admin}"
REALM="rust-web-app"
CLIENT_ID="rust-web-app"

echo "→ Authenticating with Keycloak admin console ..."
TOKEN=$(curl -sS -X POST "${KC_URL}/realms/master/protocol/openid-connect/token" \
  -d "grant_type=password" \
  -d "client_id=admin-cli" \
  -d "username=${KC_ADMIN}" \
  -d "password=${KC_PASS}" \
  | jq -r '.access_token')

if [[ -z "$TOKEN" || "$TOKEN" == "null" ]]; then
  echo "✗ Failed to authenticate with Keycloak"
  exit 1
fi

echo "→ Creating realm '${REALM}' ..."
curl -sS -X POST "${KC_URL}/admin/realms" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "realm": "'"${REALM}"'",
    "enabled": true,
    "sslRequired": "external",
    "registrationAllowed": false
  }' > /dev/null

echo "→ Creating client '${CLIENT_ID}' ..."
REALM_ID=$(curl -sS "${KC_URL}/admin/realms" \
  -H "Authorization: Bearer ${TOKEN}" \
  | jq -r ".[] | select(.realm == \"${REALM}\") | .id")

curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/clients" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "clientId": "'"${CLIENT_ID}"'",
    "enabled": true,
    "publicClient": true,
    "standardFlowEnabled": true,
    "directAccessGrantsEnabled": true,
    "implicitFlowEnabled": false,
    "serviceAccountsEnabled": false,
    "attributes": {
      "oauth2.device.authorization.grant.enabled": "false"
    }
  }' > /dev/null

echo "→ Creating 'reader' role ..."
curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/clients" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "clientId": "'"${CLIENT_ID}"'",
    "enabled": true,
    "publicClient": true,
    "standardFlowEnabled": true,
    "directAccessGrantsEnabled": true
  }' > /dev/null || true

# Get the client ID for role creation
CLIENT_UUID=$(curl -sS "${KC_URL}/admin/realms/${REALM}/clients" \
  -H "Authorization: Bearer ${TOKEN}" \
  | jq -r ".[] | select(.clientId == \"${CLIENT_ID}\") | .id" | head -1)

# Create roles
curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/clients/${CLIENT_UUID}/roles" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{"name": "read"}' > /dev/null || true

curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/clients/${CLIENT_UUID}/roles" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{"name": "write"}' > /dev/null || true

echo "→ Creating test user 'reader' with 'read' role ..."
# Create reader user
curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/users" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "username": "reader",
    "enabled": true,
    "email": "reader@example.com",
    "credentials": [{"type": "password", "value": "reader-pass", "temporary": false}]
  }' > /dev/null

READER_UUID=$(curl -sS "${KC_URL}/admin/realms/${REALM}/users?username=reader" \
  -H "Authorization: Bearer ${TOKEN}" \
  | jq -r '.[0].id')

# Assign read role to reader
READER_ROLE_UUID=$(curl -sS "${KC_URL}/admin/realms/${REALM}/clients/${CLIENT_UUID}/roles/read" \
  -H "Authorization: Bearer ${TOKEN}" \
  | jq -r '.id')

curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/users/${READER_UUID}/role-mappings/applications/${CLIENT_UUID}" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '[{"id": "'"${READER_ROLE_UUID}"'", "name": "read"}]' > /dev/null

echo "→ Creating test user 'writer' with 'read' and 'write' roles ..."
# Create writer user
curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/users" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '{
    "username": "writer",
    "enabled": true,
    "email": "writer@example.com",
    "credentials": [{"type": "password", "value": "writer-pass", "temporary": false}]
  }' > /dev/null

WRITER_UUID=$(curl -sS "${KC_URL}/admin/realms/${REALM}/users?username=writer" \
  -H "Authorization: Bearer ${TOKEN}" \
  | jq -r '.[0].id')

# Assign both roles to writer
READ_ROLE_UUID=$(curl -sS "${KC_URL}/admin/realms/${REALM}/clients/${CLIENT_UUID}/roles/read" \
  -H "Authorization: Bearer ${TOKEN}" \
  | jq -r '.id')

WRITE_ROLE_UUID=$(curl -sS "${KC_URL}/admin/realms/${REALM}/clients/${CLIENT_UUID}/roles/write" \
  -H "Authorization: Bearer ${TOKEN}" \
  | jq -r '.id')

curl -sS -X POST "${KC_URL}/admin/realms/${REALM}/users/${WRITER_UUID}/role-mappings/applications/${CLIENT_UUID}" \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer ${TOKEN}" \
  -d '[{"id": "'"${READ_ROLE_UUID}"'", "name": "read"}, {"id": "'"${WRITE_ROLE_UUID}"'", "name": "write"}]' > /dev/null

echo "→ Keycloak setup complete!"
echo ""
echo "Test credentials:"
echo "  reader: reader / reader-pass (read scope only)"
echo "  writer: writer / writer-pass (read + write scopes)"
echo ""
echo "To get a token for testing:"
echo "  curl -X POST ${KC_URL}/realms/${REALM}/protocol/openid-connect/token \\"
echo "    -d 'grant_type=password' \\"
echo "    -d 'client_id=${CLIENT_ID}' \\"
echo "    -d 'username=writer' \\"
echo "    -d 'password=writer-pass'"
