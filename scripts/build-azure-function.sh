#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="${ROOT_DIR}/dist/azure-function-app"
ZIP_PATH="${ROOT_DIR}/dist/iroh-vpn-proxy-function-app.zip"
BINARY_NAME="server"

mkdir -p "${DIST_DIR}/IrohTicket" "${DIST_DIR}/KeepAlive"


cat > "${DIST_DIR}/host.json" <<EOF
{
  "version": "2.0",
  "logging": {
    "applicationInsights": {
      "samplingSettings": {
        "isEnabled": true,
        "excludedTypes": "Request"
      }
    }
  },
  "extensions": {
    "http": {
      "routePrefix": ""
    }
  },
  "extensionBundle": {
    "id": "Microsoft.Azure.Functions.ExtensionBundle",
    "version": "[4.*, 5.0.0)"
  },
  "customHandler": {
    "description": {
      "defaultExecutablePath": "${BINARY_NAME}",
      "workingDirectory": "",
      "arguments": []
    },
    "enableForwardingHttpRequest": true
  }
}
EOF

cat > "${DIST_DIR}/IrohTicket/function.json" <<EOF
{
  "scriptFile": "../${BINARY_NAME}",
  "bindings": [
    {
      "authLevel": "anonymous",
      "type": "httpTrigger",
      "direction": "in",
      "name": "req",
      "methods": ["get"],
      "route": "iroh/ticket"
    },
    {
      "type": "http",
      "direction": "out",
      "name": "res"
    }
  ]
}
EOF

cat > "${DIST_DIR}/KeepAlive/function.json" <<EOF
{
  "scriptFile": "../${BINARY_NAME}",
  "bindings": [
    {
      "authLevel": "anonymous",
      "type": "httpTrigger",
      "direction": "in",
      "name": "req",
      "methods": ["get"],
      "route": "keep_alive"
    },
    {
      "type": "http",
      "direction": "out",
      "name": "res"
    }
  ]
}
EOF

cargo build --release --bin "${BINARY_NAME}" --target x86_64-unknown-linux-musl
cp "${ROOT_DIR}/target/x86_64-unknown-linux-musl/release/${BINARY_NAME}" "${DIST_DIR}/${BINARY_NAME}"


rm -f "${ZIP_PATH}"
(cd "${DIST_DIR}" && zip -r "${ZIP_PATH}" . >/dev/null)

echo "Built Azure Function package at ${ZIP_PATH}"
