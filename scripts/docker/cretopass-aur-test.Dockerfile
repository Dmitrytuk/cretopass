FROM archlinux:base-devel

RUN pacman -Syu --noconfirm && \
    pacman -S --noconfirm \
        rustup \
        namcap \
        git \
        desktop-file-utils \
        hicolor-icon-theme \
        gtk-update-icon-cache && \
    rm -rf /var/cache/pacman/pkg/*

RUN useradd -m builder && \
    echo "builder ALL=(ALL) NOPASSWD: ALL" >> /etc/sudoers

USER builder
WORKDIR /home/builder

RUN rustup toolchain install 1.92.0 && \
    rustup default 1.92.0