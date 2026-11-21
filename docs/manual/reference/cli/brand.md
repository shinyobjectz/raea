---
title: "raea brand"
summary: "Read a brand kit from a website on this machine: colours, fonts and logos into brand/ and BRAND.md."
order: 29
sources: ["engine/cli/src/brand.ts", "engine/cli/src/scrape.ts"]
---

`raea brand` reads a company's website from this machine and writes the project's brand kit: `brand/brand.css` (its
colours and fonts as CSS variables), `brand/brand.json`, `BRAND.md` (what it found, in words) and the logos in
`media/brand/`. When a plain fetch sees too little, as on a site that draws itself with JavaScript, it says so, and
an agent can read the site in its browser instead.

```sh
raea brand acme.com
raea brand https://acme.com/shop
```

<!-- generated:begin by tools/manual-ref/cli.ts from `raea help --json`; edit only the flag descriptions -->

## Usage

```sh
raea brand DOMAIN|URL   (the brand kit read from its website on this machine: brand/brand.css, brand/brand.json, BRAND.md, logos in media/brand; says when a plain fetch saw too little)
```

Headless: it needs nothing running and opens no window. Takes up to 1 argument besides its flags.

<!-- generated:end -->
