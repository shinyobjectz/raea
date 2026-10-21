#!/bin/bash
# convert every download into assets/<name>/<name>.glb.  usage: convert_all.sh DL_DIR [name...]
DL=$1; shift
A=~/raea/apps/void/brand/rome/assets
T=$(dirname "$0")
args_for() { case $1 in
  caesar_bust_fitzwilliam)      echo "--height 0.275";;
  caesar_bust_leiden)           echo "--height 0.315";;
  marcus_aurelius_bust)         echo "--height 0.606";;
  hadrian_statue)               echo "--height 2.3";;
  nero_head)                    echo "--height 0.40";;
  augustus_prima_porta)         echo "--height 2.233 --marble";;
  brutus_capitoline_bust)       echo "--height 0.806 --marble";;
  antoninus_pius_toga_bust)     echo "--height 0.895 --marble";;
  colosseum)                    echo "--length 189";;
  colosseum_ruin)               echo "--height 48";;
  pantheon)                     echo "--height 43.3";;
  triumphal_arch_palmyra)       echo "--length 25";;
  roman_temple)                 echo "--keep-scale";;
  maison_carree_temple)         echo "--keep-scale";;
  temple_of_vespasian_columns)  echo "--keep-scale";;
  galea_helmet)                 echo "--keep-scale";;
  gladius)                      echo "--length 0.75";;
  scutum_shield)                echo "--keep-scale";;
  caesar_denarius_coin)         echo "--length 0.019";;
  amphora_dressel_1a)           echo "--height 1.1";;
  oil_lamp_lucerna)             echo "--length 0.12";;
  marcus_aurelius_equestrian)   echo "--height 4.24";;
  legionary_statue)             echo "--height 2.0";;
esac; }
names=("$@"); [ ${#names[@]} -eq 0 ] && names=($(ls "$DL"))
for n in "${names[@]}"; do
  f=$(ls "$DL/$n"/source.* | head -1)
  extra=$(cat "$T/overrides/$n" 2>/dev/null)
  blender -b --factory-startup -P "$T/convert.py" -- "$f" "$A/$n/$n.glb" $(args_for $n) $extra 2>&1 | grep -E '^RESULT|Error|Traceback' | sed "s|^RESULT|$n|"
done
