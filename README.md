# `kathe`

Redesign of `kathe`, together with the people of [NCSC-NL.](https://github.com/ncsc-nl)

## A short history of `kathe`  

`kathe` started as a pet project at [KPN-CISO](https://github.com/kpn-ciso) to implement a way to do quick and dirty enrichment of our CTI with a graph of binaries (usually malware) of sufficient similarity to samples from investigations. The main problems to be solved were not having to do any-to-any comparisons on large data sets, and having an API we could talk to.

Although `kathe` does this just fine, it does suffer from some unfortunate design mistakes, overkill, feature creep and tons of slow Python. Not to mention my coding skills, which were –and still are– just barely functioning hackery.

This is the latest attempt, with the `kathe-cli` tool currently being written in [Rust](https://www.rust-lang.org/), and the backend/API/web interface in [Golang](https://go.dev/). Why two languages? Because of personal preferences. ¯\\_(ツ)_/¯

This *cli* tool currently has all kinds of redis functionality which we'll probably strip out, as the backend/db is being redesigned with PostgreSQL. The [TSV output](https://github.com/avuko/kathe-tsv) will likely be stashed into the kathe DB via an API. 

``` shell
./kathe --help
kathe 0.5
avuko
kathe is a tool to correlate inputs based on ssdeep similarity
TSV fields: "inputname"\t"md5"\t"sha1"\t"sha256"\t"ssdeep"\t"context[,context,...]"
named after Katherine Johnson of NASA fame.

USAGE:
    kathe [OPTIONS] --context <context> <--filepath <filepath>|--inputtsv>

OPTIONS:
    -a, --auth <auth>              [default: redis]
    -c, --context <context>        list,of,contexts
    -d, --dbnumber <dbnumber>      [default: 7]
    -f, --filepath <filepath>      Path to file to be parsed
    -h, --help                     Print help information
    -i, --inputtsv                 Parse a TSV from STDIN
    -p, --port <port>              [default: 6379]
    -r, --redishost <redishost>    [default: 127.0.0.1]
    -V, --version                  Print version information
```

This tool can also be used to stash the TSV in a redis store, but for now that is not used.

```shell
ls -1 Block.0095/ |wc -l
40000


time find Block.0095/ -type f | while read line; do ./kathe -c vxug,block.0095 -f "${line}" >> test.tsv ;done

[...]

real	10m4.346s
user	8m39.796s
sys	1m48.942s
```

Not too shabby for 40,000 samples, totalling 15.4 GB



To set up Rust:

```shell
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

To build for prod, in the repo:

```shell
cargo build --release

file target/release/kathe
target/release/kathe: ELF 64-bit LSB pie executable, x86-64, version 1 (SYSV), dynamically linked, interpreter /lib64/ld-linux-x86-64.so.2, BuildID[sha1]=a0a8523dd764ee6eb8c50dbbc96cbc26d329796b, for GNU/Linux 3.2.0, with debug_info, not stripped
```

