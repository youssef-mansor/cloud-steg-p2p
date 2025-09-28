# Base image
FROM ubuntu:22.04

# Avoid interactive prompts during build
ENV DEBIAN_FRONTEND=noninteractive

# Update apt and install Python + nano
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    nano \
    tmux \
    iproute2 \
 && rm -rf /var/lib/apt/lists/*

# Default command: keep container running
CMD ["sleep", "infinity"]
