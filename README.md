# Iroh Proxy

This project is a two part vpn/proxy server that uses Iroh to tunnel from the
entry client node to the exit server node.

By using Iroh, we can use NAT UDP hole-punching to create a secure tunnel between
the two nodes, even when either/both are being a NAT.

This is extremely useful for hosting the exit server in constrained environments
such as on FAAS infrastructure.

As long as the FAAS infrastructure allows a HTTP request in, we can get the Iroh
ticket to connect to the exit server from the entry node.

As Iroh is underpinned by QUIC, this tunnel is TLS encrypted, and can be used to
bypass georestrictions or country-specific firewalls.