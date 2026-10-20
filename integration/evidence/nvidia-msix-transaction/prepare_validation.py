from pathlib import Path
import subprocess
R=Path('/workspace/asterinas-gpui'); O=R/'evidence/nvidia-msix-transaction'; P=R/'evidence/nvidia-irq-callbacks'
(O/'tracked-index-before.txt').write_bytes(subprocess.check_output(['git','ls-files','-s'],cwd=R/'asterinas'))
s=(P/'run_cpu_boot.py').read_text().replace('evidence/nvidia-irq-callbacks/cpu-boot','evidence/nvidia-msix-transaction/cpu-boot')
(O/'run_cpu_boot.py').write_text(s)
s=(P/'make_patch.py').read_text().replace("OUT = ROOT / 'evidence/nvidia-irq-callbacks'", "OUT = ROOT / 'evidence/nvidia-msix-transaction'").replace("PATCH = ROOT / 'local-prs/asterinas-nvidia-irq-callbacks.patch'", "PATCH = ROOT / 'local-prs/asterinas-nvidia-msix-transaction.patch'")
start=s.index('NAMES = '); end=s.index('\ndef sha',start)
s=s[:start]+'''NAMES = sorted({str(p.relative_to(OUT/'before')) for p in (OUT/'before').rglob('*') if p.is_file()} | {
    'kernel/core/comps/pci/src/msix_transaction.rs',
    'kernel/core/comps/pci/src/msix_tests.rs',
})
'''+s[end:]
s=s.replace('All twelve Asterinas patches through asterinas-nvidia-multifunction.patch','All thirteen Asterinas patches through asterinas-nvidia-irq-callbacks.patch')
(O/'make_patch.py').write_text(s)
s=(P/'validate_stack.py').read_text().replace("OUT=ROOT/'evidence/nvidia-irq-callbacks'", "OUT=ROOT/'evidence/nvidia-msix-transaction'").replace("    'asterinas-nvidia-irq-callbacks.patch',", "    'asterinas-nvidia-irq-callbacks.patch',\n    'asterinas-nvidia-msix-transaction.patch',")
(O/'validate_stack.py').write_text(s)
print('Runners prepared')
