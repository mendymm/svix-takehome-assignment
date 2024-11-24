FROM ubuntu:plucky

WORKDIR /app

RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    # Clean up
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

# todo(production): use cargo chef for docker image, but for now this is faster. (since i can use my target dir cache)
COPY  target/release/svix-takehome-assignment svix-takehome-assignment

ENTRYPOINT ["./svix-takehome-assignment"]