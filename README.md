# `kathe`

Redesign of `kathe`, together with the people of NCSC-NL.

## A short history of `kathe`  

`kathe` started as a pet project at [KPN-CISO](https://github.com/kpn-ciso) to implement a quick and dirty enrichment of our CTI with similar binaries. The main problems to be solved were not having to do any-to-any comparisons on large data sets, and having an API we could talk to.

Although `kathe` does this just fine, it does suffer from some unfortunate design mistakes, overkill, feature creep and tons of slow Python. Not to mention my coding skills, which were –and still are– just barely functioning hackery.

This is the latest attempt, with the `kathe-cli` tool currently being written in [Rust](https://www.rust-lang.org/), and the backend/API/web interface in [Golang](https://go.dev/). Why two languages? Because of personal preferences. ¯\\_(ツ)_/¯

So, although `kathe-cli` already exists (sort of), for now my focus will be this document. Documenting every design choice and data-type. Because I'd rather be caught before my mistakes, love of overkill, and feature creep again get the better of me.

## keys, types and context

We are going to keep the design as simple as possible. To that end, we only have `context`, which will include what is currently `inputname`, `sha256` and `context` and `ssdeep`, which is used to create the graphs. Because we want to be able to pivot on any `context`, and we currently don't know what contexts we'll be adding in the future, every `context` will be a `zset`. The key name will be the unique context, the key value will be a `zset` (a non repeating collections of `Strings`, where every member has an associated score) of `ssdeep` hashes.

To keep track (and literally: count) of what's in the db, we'll have two indexes, `index:context`, and `index:ssdeep`, both also `zset` types. 

> [avuko] I wondered whether both are necessary. I can see the upside of having a count of contexts, so we can see how often we've seen a filename, how many entries are from a particular source, or how often we've seen a specific `sha256` in different sources. I don't think we'll want to know how often we've seen a particular ssdeep, but maybe we might want to somewhere down the line. And we do want to have a count of the number of unique `ssdeep` hashes, so an index is needed anyway. And if we create an `set` index, why not a `zset`, so we have that info when we want it?   

| key name      | value type               | redis type | example                                                      | context                                                      |
| ------------- | ------------------------ | ---------- | ------------------------------------------------------------ | ------------------------------------------------------------ |
| timestamp     | String [epoch.as_micros] | key        | `1651605418685632`                                           | Used to timestamp latest additions to the database, and be able to identify and remove stale caches. |
| index:context | String + count           | zset       | `1) "fdddbbc09972a8da879209f8b45796b4343ffd8c74ae8e56bfe78aebc710777b`<br/>`2) "1"`<br/>`3) "2019-05-11"`<br/>`4) "1"`<br/>`5) "win.revil"`<br/>`6) "52"` |Used to keep count of all added contexts. Whatever we use to standardise/defang the strings while adding, should also be used before a lookup/search, so we don't miss things. Think uppercase/lowercase hashes, spaces etc.|
|index:ssdeep|String + count|zset||Everything is stored as "context" under a ssdeep, but we want to make `sha256 ` special, so we can more easily pivot and search.|
| |                                                              ||||
|   |                          |            |                                                              |                                                              |
|               |                          |            |                                                              |                                                              |

