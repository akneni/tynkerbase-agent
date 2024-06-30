import subprocess
import shutil
import os

tyb_path = "/usr/share/tynkerbase-agent"
if os.path.exists(tyb_path):
    shutil.rmtree(tyb_path)
if not os.path.exists(tyb_path):
    os.makedirs(tyb_path)

child = subprocess.run("git clone https://github.com/akneni/tynkerbase-agent.git /usr/share/tynkerbase-agent".split())

if os.path.exists('/usr/local/bin/tyb_agent'):
    os.remove('/usr/local/bin/tyb_agent')
os.symlink('/usr/share/tynkerbase-agent/target/release/tynkerbase-agent', '/usr/local/bin/tyb_agent')