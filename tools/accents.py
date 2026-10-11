#!/usr/bin/env python3
"""Restore French accents in the displayed strings of data/**/*.ron (fields name, text, ideology).

The generators write plain ASCII; this runs after them (gen_content, gen_colony, gen_science,
gen_scale, then accents.py). It is idempotent: already accented words are never touched. Ambiguous
words (a/à, ou/où, des/dès, releve, blesse, brule, isole, epuise...) are left alone on purpose.
"""
import glob
import os
import re

WORDS = """
affilee:affilée affole:affolé amitie:amitié anxiete:anxiété appliquee:appliquée apres:après
arret:arrêt arreter:arrêter atmospherique:atmosphérique aupres:auprès aussitot:aussitôt
avancee:avancée avancees:avancées avances:avancés batiment:bâtiment brulure:brûlure
brulures:brûlures cafe:café cartographiee:cartographiée cereales:céréales cerebrale:cérébrale
ceramique:céramique ceramiques:céramiques cholera:choléra cinetiques:cinétiques comete:comète
colere:colère colerique:colérique contrariete:contrariété controle:contrôle corvees:corvées
credits:crédits decennies:décennies dechire:déchire declenche:déclenche decoder:décoder
decompression:décompression decouvre:découvre defectueux:défectueux defense:défense
delicat:délicat demonte:démonte demontre:démontre densite:densité deployee:déployée depot:dépôt
depression:dépression depressurisation:dépressurisation derivante:dérivante derive:dérive
derriere:derrière desespoir:désespoir detachees:détachées deuterium:deutérium
developpe:développe discretement:discrètement dome:dôme donnees:données echantillon:échantillon
echantillons:échantillons echecs:échecs echelle:échelle echos:échos ecole:école ecologie:écologie
economise:économise economiser:économiser ecope:écope ecoutee:écoutée electrise:électrise
electrolyse:électrolyse electromagnetique:électromagnétique electronique:électronique emet:émet
energetique:énergétique energie:énergie enterite:entérite entraine:entraîne
entrainement:entraînement entrepot:entrepôt epave:épave epouvantable:épouvantable
epuisement:épuisement equipage:équipage eternue:éternue etoile:étoile etoiles:étoiles
etrange:étrange etudie:étudie evacuation:évacuation experience:expérience fete:fête
fievre:fièvre fidelite:fidélité fiscalite:fiscalité foret:forêt fregate:frégate gateau:gâteau
geant:géant generateur:générateur genereux:généreux geothermique:géothermique grace:grâce
gravite:gravité hauturiere:hauturière hegemonie:hégémonie helium:hélium hepatite:hépatite
hydrogene:hydrogène idee:idée idees:idées ingenierie:ingénierie ingenieur:ingénieur
inquiete:inquiète journee:journée lactee:lactée legeres:légères leviathan:léviathan
magnesium:magnésium magnetique:magnétique maraichage:maraîchage malgre:malgré
materiaux:matériaux materiel:matériel mecanicien:mécanicien medecin:médecin medecine:médecine
medical:médical medicaments:médicaments medicinales:médicinales meme:même
meningite:méningite metallique:métallique methane:méthane methanisation:méthanisation
methodes:méthodes meticuleux:méticuleux metier:métier miniere:minière
nanomateriaux:nanomatériaux nebuleuse:nébuleuse negligence:négligence negociation:négociation
oxygene:oxygène petrochimie:pétrochimie piece:pièce pieces:pièces piegees:piégées
planete:planète pole:pôle polymeres:polymères precision:précision precoce:précoce
prefere:préfère preparent:préparent pres:près priere:prière prolongee:prolongée
quatrieme:quatrième reacteur:réacteur reaction:réaction reclame:réclame reclamee:réclamée
recolte:récolte recoltes:récoltes recupere:récupère reecoute:réécoute reference:référence
reglement:règlement regolithe:régolithe regularite:régularité regulier:régulier repere:repère
reparation:réparation reparations:réparations reparer:réparer republique:république
reseaux:réseaux reservoir:réservoir residentiel:résidentiel reveille:réveille risquee:risquée
separation:séparation septicemie:septicémie serie:série severe:sévère soiree:soirée
superieur:supérieur supplementaires:supplémentaires symptomes:symptômes synthese:synthèse
synthetique:synthétique synthetiques:synthétiques tempete:tempête tete:tête theorie:théorie
tres:très typhoide:typhoïde vegetales:végétales xenologie:xénologie zephyr:zéphyr
"""

MAP = dict(p.split(":") for p in WORDS.split())
FIELD = re.compile(r'\b(name|text|ideology)(\s*:\s*)"([^"]*)"')
WORD = re.compile(r"[A-Za-z]+")


def fix_word(m):
    w = m.group(0)
    new = MAP.get(w.lower())
    if new is None:
        return w
    if w.isupper() and len(w) > 1:
        return new.upper()
    if w[0].isupper():
        return new[0].upper() + new[1:]
    return new


def fix_text(s):
    return WORD.sub(fix_word, s)


def main():
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data")
    changed = 0
    for path in glob.glob(os.path.join(root, "**", "*.ron"), recursive=True):
        with open(path, encoding="utf-8", newline="") as f:
            src = f.read()
        out = FIELD.sub(lambda m: f'{m.group(1)}{m.group(2)}"{fix_text(m.group(3))}"', src)
        if out != src:
            with open(path, "w", encoding="utf-8", newline="") as f:
                f.write(out)
            changed += 1
    print(f"accents: {changed} file(s) updated")


if __name__ == "__main__":
    main()
