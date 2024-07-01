# TynkerBase - Agent App
**The cloud, in the palm of your hand**

## Overview
The Agent App is a core component of the TynkerBase project. It runs on various devices, enabling the deployment and management of cloud services on inexpensive hardware like Raspberry Pi, repurposed laptops, or even smartphones (work in progress). The Agent App allows users to push code to these devices, dockerize it, and deploy it seamlessly.

## Features

- **Deployment Automation**: Automatically deploys applications using Docker.
- **Device Management**: Manages multiple devices and ensures smooth operation.
- **User-Friendly**: Simple setup and configuration process.
- **Educational**: Helps new developers learn about cloud services deployment and management.

## Getting Started
- TBD

### Prerequisites
- **Operating System**: Linux, Windows though WSL
- **Dependencies**: 
    - Docker
    - OpenSSL
    - libssl-dev
    - pkg-config

### Installation
- Install tynkerbase agent. This will only work for linux x86; for linux ARM, you will need to build from source. 
```bash
curl https://raw.githubusercontent.com/akneni/tynkerbase-agent/master/installation/install.py -o tynkerbase-install.py
sudo python3 tynkerbase-install.py
```

- Install and build form source (you will need cargo for this)
```bash
git clone https://github.com/akneni/tynkerbase-agent.git
cd tynkerbase-agent
cargo build --release
cd ..
sudo mv ./tynkerbase-agent /usr/share 
sudo ln -sf /usr/share/tynkerbase-agent/target/release/tynkerbase-agent /usr/local/bin/tyb_agent
```

- Uninstall
```bash
curl https://raw.githubusercontent.com/akneni/tynkerbase-agent/master/installation/uninstall.py -o tynkerbase-uninstall.py
sudo python3 tynkerbase-uninstall.py
```