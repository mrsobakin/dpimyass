pkgname=dpimyass
pkgver=0.3.0
pkgrel=1
pkgdesc="Simple UDP proxy for bypassing the DPI"
arch=('x86_64')
license=('GPL v3')
makedepends=('rust')
depends=('systemd')

build() {
    cargo build --release
}

package() {
    install -Dm755 "$srcdir/../target/release/dpimyass" "$pkgdir/usr/bin/dpimyass"
    install -Dm644 "$srcdir/../dpimyass.service" "$pkgdir/usr/lib/systemd/system/dpimyass.service"
    install -Dm600 "$srcdir/../config.toml" "$pkgdir/etc/dpimyass/config.toml"
}
