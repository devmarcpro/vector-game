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
        f' (id:"clone_{i}", name:"Clone : {n}", category:"organes", density:1050.0, value:{float(v) * 1.5:.1f}),'
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
gear = re.findall(
    r'\(id:"(\w+)", name:"([^"]+)", slot:"\w+".*?value:([\d.]+)',
    (root / "data" / "gear.ron").read_text(encoding="utf-8"),
)
for i, n, v in gear:
    out.append(
        f' (id:"gear_{i}", name:"Equipement : {n}", category:"equipement", density:1800.0, value:{float(v):.1f}),'
    )
out.append(
    ' (id:"mutagen", name:"Mutagene", category:"chimie", density:1200.0, value:900.0),'
)
out.append("]")
(root / "data" / "resources_organs.ron").write_bytes(("\n".join(out) + "\n").encode("utf-8"))
print(len(organs) * 3 + len(cyber) + len(gear) + 1, "resources")
