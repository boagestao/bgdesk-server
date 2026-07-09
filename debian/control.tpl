Source: bgdesk-server
Section: net
Priority: optional
Maintainer: open-trade <info@bgdesk.com>
Build-Depends: debhelper (>= 10), pkg-config
Standards-Version: 4.5.0
Homepage: https://bgdesk.com/

Package: bgdesk-server-hbbs
Architecture: {{ ARCH }}
Depends: systemd ${misc:Depends}
Description: BGDesk server
 Self-host your own BGDesk server, it is free and open source.

Package: bgdesk-server-hbbr
Architecture: {{ ARCH }}
Depends: systemd ${misc:Depends}
Description: BGDesk server
 Self-host your own BGDesk server, it is free and open source.
 This package contains the BGDesk relay server.

Package: bgdesk-server-utils
Architecture: {{ ARCH }}
Depends: ${misc:Depends}
Description: BGDesk server
 Self-host your own BGDesk server, it is free and open source.
 This package contains the bgdesk-utils binary.
