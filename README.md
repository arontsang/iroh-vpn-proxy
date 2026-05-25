# Iroh Proxy

## What is this is for

This is a lightweight project that is designed to be deployed to Azure's FAAS intrastructure to provide the user
with a on-demand scale to zero VPN/HTTP Proxy for use with circumventing geolocation and political firewall
restrictions, with minimal cost (scale to zero).

The project comes in two parts. A FAAS binary that is deployed to Azure's Functions. This allows clients to
dial in using HTTP to get a Iroh ticket, which allows the creation of a Quic connection behind a NAT.

As Iroh is underpinned by QUIC, this tunnel is TLS encrypted, and can be used to
bypass georestrictions or country-specific firewalls.

As long as the FAAS infrastructure allows a HTTP request in, we can get the Iroh
ticket to connect to the exit server from the entry node.

## How this works

### The FaaS binary contains:
  - A UDP NAT hole punch
  - A QUIC server
  - A meta data http server (to act as the side channel for UDP hole punching)
  - A HTTP forward proxy (behind the QUIC server)

On startup, the FaaS application does a UDP hole punch to get a publicly accessible port.

It then advertises the UDP port on it HTTP server.

On incoming QUIC connections, we treat the bi-directional streams as HTTP/1.1 forward proxy requests.

### The client binary:
  - TCP Listener/forwarder
  - Iroh UDP hole puncher
  - A QUIC client
  - Http Keep Alive component

The client on receiving an incoming TCP request, will the UDP hole-punch into the FaaS service
using the HTTP service to setup the side channel (getting the public socket address, and initiating
UDP hole punch packets).

We then open up a Quic connection, then tunnel all incoming TCP connections through Quic.

Additionally we also start a task to keep the FaaS alive until all connections are drained.

## What is this is for

This is a lightweight project that is designed to be deployed to Azure's FAAS intrastructure to provide the user
with a on-demand scale to zero VPN/HTTP Proxy for use with circumventing geolocation and political firewall
restrictions, with minimal cost (scale to zero).

This should easily be able to run within the free tier of Azure's Functions for approximately 8 days per month 
(approximately 1/4 of the time).

## Why does this exist?

This is a lightweight project that is designed to be deployed to Azure's FAAS intrastructure to provide the user
with a on-demand scale to zero VPN/HTTP Proxy for use with circumventing geolocation and political firewall
restrictions, with minimal cost (scale to zero).

Originally this project used a userland TCP/IP stack and wireguard for tunneling the proxy requests. However,
I found that I quickly hit a performance bottleneck between the TCP/IP stack and the UDP socket, which I could not 
solve easily.

