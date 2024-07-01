import shutil
import os

if os.path.exists('/usr/local/bin/tyb_agent'):
    os.remove('/usr/local/bin/tyb_agent')

tyb_path = "/usr/share/tynkerbase-agent"
if os.path.exists(tyb_path):
    shutil.rmtree(tyb_path)

if not os.path.exists('/tyb-root'):
    exit(0)

res = input('Do you also want to uninstall all your tynkerbase projects? (y/n)  ')
if res.strip().lower() in ('y', 'yes'):
    shutil.rmtree('/tyb-root')
