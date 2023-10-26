# Litt
Literature tool for searching all PDFs in a directory.

### tl;dr
**Installing**:
```
wget https://github.com/georgbuechner/litt/releases/download/v1.0.1/litt-ubuntu-latest 
chmod +x litt-ubuntu-latest
cp -f litt-ubuntu-latest /usr/local/bin/litt
```
(Replace version and platform accordingly)

**Usage**:
```
litt <index-name> -i <path-to-documents>
litt <index-name> <search-term>
```

https://github.com/georgbuechner/litt/assets/24663156/4e9b069d-8806-4a4f-9efb-02176910512f


# Table of contents
1. [litt](#litt)
2. [installation](#installation)
	1. [requirements](#requirements)
	2. [pre-built binaries](#prebuilt-binaries)
1. [usage](#usage)
    1. [create new Index](#create)
    2. [updating, reloading, deleting](#updating)
    3. [searching](#searching)
		- [exact matching](#exact-matching)
		- [fuzzy matching](#fuzzy-matching)
3. [Acknowledgements](#acknowledgements)


# Litt
Literature tool for searching all PDFs in a directory.

## Installation 

### Requirements
The command-line tool [pdftotext](https://www.xpdfreader.com/download.html) should be available on your system.

Also, we advise to install [zathura](https://pwmt.org/projects/zathura/installation/), a very lightweight pdf reader. If zathura is availible we can open the selected pdf (`litt <num>`) on the selected page and with the search term highlighted.

### Pre-built binaries.<a name="prebuilt-binaries"></a>
There are pre-built binaries available for Windows, Linux and MacOS. Simply
download the binary (see: *Releases*) and you are ready to go. It is advised to add `litt` to path. 

#### Linux 
This would be a way to go for Linux:
```
wget https://github.com/georgbuechner/litt/releases/download/v1.0.1/litt-ubuntu-latest 
chmod +x litt-ubuntu-latest
cp -f litt-ubuntu-latest /usr/local/bin/litt
```
- `wget` downloads the file (*make sure to change the version-number to the latest
version*). 
- `chmod +x` grants permission to execute `litt`. (This must be run as `sudo`
  user)
- `cp -f litt-ubuntu-latest /usr/local/bin/litt` makes `litt` available system wide under the name `litt`. 

Verify by running `litt --version`. It should show something like: 
```
litt 1.0.1
```

#### MacOS
Installation should work similar to Linux: 
```
wget https://github.com/georgbuechner/litt/releases/download/v1.0.1/litt-macOS-latest
chmod +x litt-macOS-latest
cp -f litt-macOS-latest /usr/local/bin/litt
```

#### Windows 
Honestly, I don't really know. After downloading the file `litt` should be added
to path. This guide gives some explanation on how to do that: 
https://windowsloop.com/how-to-add-to-windows-path/

## Usage 

![basic functionality](images/basic_functionality.png)

### Create a new `litt`-index <a name="create"></a>
This is how you create a new index:
```
litt <index-name> -i <path-to-documents>
```
Assuming you have some documents stored at `Documets/Literature/books/` which you
would like to index, you can do this as following: 
```
litt books -i Documets/Literature/books/
```
*NOTE:*
- *the index-name can be any name. It need not match with the directory name.*
- *any relative path is automatically changed to an absolute path* (e.i.
  `Docuemts/Literature/books/` to `/home/<user>/Docuemts/Literature/books/`)

### Updating, reloading, deleting an existing index<a name="updating"></a>
To see all existing indices, type: 
```
litt -l 
> Currently available indices:
 - ("books", "/home/<user>/Documents/Literature/books/")
 - ("papers", "/home/<user>/Documents/papers/")
 - ("notes", "/home/<user>/Documents/notes/")
```
You can then update an existing index: `litt books -u` which is usually very
fast, but might not track all changes made to *existing* documents and will
never track *deleted* documents. Use `litt books --reload` to fully reload the
index. This might take a while. 

To delete an index, type: `litt books -r`

### Searching 
In general you search like this: 
```
litt <index-name> <search-term>
```
If your search term is more than one word, you should add quotations: `litt <index-name> '<term1 term2 ...>'`

Use `--offset` and `--limit` to show more results. (Default shows the top ten
results. `--offset 10` shows the first 10 to 20 results. `--offset 10 --limit 50`
shows the first 10 to 60 results).

![offset-limit example](images/offset-limit_example.png)

Use `litt <num>` to open a document (num refers to the number in brackets, f.e.
`- [1] p. XXX: ...`)

![simple example](images/simple_example.png)

### Exact matching 
You can search for multiple words, the following will give the same result
```
litt books "Tulpen Rosen" 
litt books "Tulpen OR Rosen" 
```
And show all documents (pages) which contain the term `Tuplen` *or* `Rosen`. This 
```
litt books "Tulpen AND Rosen" 
```
will only show documents (pages) which contain *both* the term `Tulpen` *and*
the term `Rosen`.

You may also combine: 
```
litt books "(Tulpen AND Rosen) OR Narzisse" 
```

You can also search for fixed phrases: 
```
litt books '"Tulpen Narzisse"'
```
Or: 
```
litt books '"Tulpen Narzisse"~1'
```
which will also match f.e. `Tulpen wie Narzisse`.

![phrase example](images/phrase_example.png)

Finally, you can find partial matches with: 
```
litt books '"Tulpen Narz"*'
```

A detailed listing of possible queries and also limitations can be found on the
`tantivy` page: https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html

### Fuzzy Matching 
Fuzzy matching can be helpful to find partial matches on single words (e.i.
match `nazis` when searching for `nazi`).
But also to correct typos or bad scans (e.i. find `nacis` when searching for
`nazis`). This can be done by using the `fuzzy` flag:
```
litt books nazis --fuzzy
```

![fuzzy example](images/fuzzy_example.png)

You can also specify the distance the search and matched term may have
(default=2): 
```
litt books nazis --fuzzy --distance 2 
```

You may also search for multiple words:
```
litt books 'Tulp Narz' --fuzzy
```

*Note*
- working with phrases (`litt books '"Tulpen Narzisse"~1'`) or `AND`/`OR`
does not work. 
- no preview can be shown when using fuzzy search.
- fuzzy matching only works on the body


## Acknowledgements

### Dependencies
We explicitly want to thank all the great developers who help write and maintain the
awesome libraries makeing `litt` possible:
- [tantivy](https://github.com/quickwit-oss/tantivy) the mind blowing full-text search engine library
- [clap](https://github.com/clap-rs/clap) the beautiful command line argument
  parser.
- [serde-json](https://github.com/serde-rs/json) for serializing and
  deserializing JSONs
- [shellexpand](https://github.com/netvl/shellexpand) for making our lives in a
  cross-platform world easier :)
- [uuid](https://github.com/uuid-rs/uuid) how would we (uniquely) identify each other
  without you? 
- [colored](https://github.com/colored-rs/colored) for making our output more
  colorful (even though it really isn't)
- [walkdir](https://github.com/BurntSushi/walkdir) for helping us gather all
  your documents
- [pdftotext](https://www.xpdfreader.com/pdftotext-man.html) which is amazingly
  good at doing its job)

We also want to clarify, that not all dependencies use the same license as we do:

| name | license |
|------|---------|
| tantivy | MIT |
| clap | Apache-2.0, MIT |
| serde json | Apache-2.0, MIT |
| shellexpand | Apache-2.0, MIT |
| uuid | Apache-2.0, MIT |
| colored | MPL-2.0 license |
| walkdir | MIT |
