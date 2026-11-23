#!/bin/sh
# Downloads the open dictionary sources into data/raw. Safe to re-run: existing files are kept.
set -eu
cd "$(dirname "$0")/.."
RAW=data/raw
mkdir -p "$RAW/ding" "$RAW/wikdict" "$RAW/openrussian" "$RAW/freq" "$RAW/kaikki"

get() { # url dest
  if [ -s "$2" ]; then echo "have $2"; return; fi
  echo "get  $2"
  curl -fL --retry 3 -sS -o "$2.part" "$1" && mv "$2.part" "$2"
}

# Ding (TU Chemnitz), GPL — DE<->EN
get https://ftp.tu-chemnitz.de/pub/Local/urz/ding/de-en/de-en.txt.gz "$RAW/ding/de-en.txt.gz"
[ -s "$RAW/ding/de-en.txt" ] || gunzip -kc "$RAW/ding/de-en.txt.gz" > "$RAW/ding/de-en.txt"

# WikDict (Wiktionary-derived bilingual pairs), CC BY-SA
for p in de-en en-de de-ru ru-de en-ru ru-en; do
  get "https://download.wikdict.com/dictionaries/sqlite/2/$p.sqlite3" "$RAW/wikdict/$p.sqlite3"
  [ -s "$RAW/wikdict/$p.tsv" ] || sqlite3 -separator "$(printf '\t')" "$RAW/wikdict/$p.sqlite3" \
    "select replace(written_rep, char(9), ' '), replace(trans_list, char(9), ' '), max(score) from translation where is_good = 1 group by 1, 2" \
    > "$RAW/wikdict/$p.tsv"
done

# OpenRussian, CC BY-SA — RU lemmas with EN/DE translations and inflected forms
for f in nouns verbs adjectives others; do
  get "https://raw.githubusercontent.com/Badestrand/russian-dictionary/master/$f.csv" "$RAW/openrussian/$f.csv"
done
get https://raw.githubusercontent.com/Badestrand/russian-dictionary/master/LICENSE "$RAW/openrussian/LICENSE"

# Word frequency (OpenSubtitles 2018, hermitdave/FrequencyWords), CC BY-SA
for l in de en ru; do
  get "https://raw.githubusercontent.com/hermitdave/FrequencyWords/master/content/2018/$l/${l}_50k.txt" "$RAW/freq/$l.txt"
done
# Wiktionary extracts (kaikki.org), CC BY-SA — about 2.6 GB
K=https://kaikki.org
get "$K/dictionary/German/kaikki.org-dictionary-German.jsonl" "$RAW/kaikki/enwikt-de.jsonl"
get "$K/dictionary/Russian/kaikki.org-dictionary-Russian.jsonl" "$RAW/kaikki/enwikt-ru.jsonl"
get "$K/ruwiktionary/%D0%9D%D0%B5%D0%BC%D0%B5%D1%86%D0%BA%D0%B8%D0%B9/kaikki.org-dictionary-%D0%9D%D0%B5%D0%BC%D0%B5%D1%86%D0%BA%D0%B8%D0%B9.jsonl" "$RAW/kaikki/ruwikt-de.jsonl"
get "$K/ruwiktionary/%D0%90%D0%BD%D0%B3%D0%BB%D0%B8%D0%B9%D1%81%D0%BA%D0%B8%D0%B9/kaikki.org-dictionary-%D0%90%D0%BD%D0%B3%D0%BB%D0%B8%D0%B9%D1%81%D0%BA%D0%B8%D0%B9.jsonl" "$RAW/kaikki/ruwikt-en.jsonl"
echo done
