"""Generates data/resources_organs.ron (grafts, prostheses and cybernetic implants as trade goods)
from data/organs.ron and data/cybernetics.ron."""
import re
import pathlib

root = pathlib.Path(__file__).resolve().parent.parent
text = (root / "data" / "organs.ron").read_text(encoding="utf-8")
organs = re.findall(
    r'\(id:"(\w+)", name:"([^"]+)", vital:\w+, weight:[\d.]+, heal:[\d.]+, value:([\d.]+)', text
)
out = [
    "// GENERATED from data/organs.ron by tools/gen_organs.py. Grafts and prostheses as trade goods.",
    "[",
]
for i, n, v in organs:
    out.append(
        f' (id:"organ_{i}", name:"Greffon : {n}", category:"organes", density:1050.0, value:{float(v):.1f}),'
    )
for i, n, v in organs:
    out.append(
        f' (id:"prosthesis_{i}", name:"Prothèse : {n}", category:"protheses", density:3000.0, value:{float(v) * 0.4:.1f}),'
    )
cyber = re.findall(
    r'\(id:"(\w+)", name:"([^"]+)", slot:"\w+".*?value:([\d.]+)', (root / "data" / "cybernetics.ron").read_text(encoding="utf-8")
)
for i, n, v in cyber:
    out.append(
        f' (id:"cyber_{i}", name:"Implant : {n}", category:"cybernetique", density:2500.0, value:{float(v):.1f}),'
    )
out.append("]")
(root / "data" / "resources_organs.ron").write_bytes(("\n".join(out) + "\n").encode("utf-8"))
print(len(organs) * 2 + len(cyber), "resources")
