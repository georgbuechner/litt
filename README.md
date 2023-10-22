# litt
Literature tool for searching all pdfs in a directory

## Dependencies 
The command-line tool `pdftotext` should be available on your system.

## Usage 

### Create a new LITT-index 
This is how you create a new index:
```
litt <index-name> -i <path-to-documents>
```
Assuming you have some Docuemts stored at `Documets/Literature/books/` which you
would like to index, you can do this as following: 
```
litt books -i Documets/Literature/books/
```
*NOTE:*
- *the index-name can be any name. It need not match with the directory name.*
- *any relative path is automatically changed to an absolute path (e.i.
  `Docuemts/Literature/books/` to `/home/<user>/Docuemts/Literature/books/`

### Searching 
In general you search like this: 
```
litt <index-name> <search-term>
```
If your search term is more than one word, you should add quotations: `litt
<index-name> '<term1 term2 ...>`

### Exact matching 
You can search for multiple words, the following will give the same result
```
litt books "Tulpen Rosen" 
litt books "Tulpen OR Rosen" 
```
And show all docuemnts (pages) which contain the term `Tuplen` *or* `Rosen`. This 
```
litt books "Tulpen AND Rosen" 
```
will only show docuemnts (pages) whcih contain *both* the term `Tulpen` *and*
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

Finally, you can find partial matches with: 
```
litt books '"Tulpen Narz"*'
```

A detailed listing of possible queries and also limitations can be found on the
`tantivy` page: https://docs.rs/tantivy/latest/tantivy/query/struct.QueryParser.html

### Fuzzy Matching 
Fuzzy matching can be helpfull to find partial matches on single words (e.i.
match `nazis` when searching for `nazi`).
But also to correct typos or bad scans (e.i. find `nacis` when searching for
`nazis`). This can be done by using the `fuzzy` flag:
```
litt books nazis --fuzzy
```
You can also specify the distance the search and matched term may have
(default=2): 
```
litt books nazis --fuzzy --distance 2 
```

You may also search for multiple words:
```
litt books 'Tulp Narz' --fuzzy
```

However, working with frases (`litt books '"Tulpen Narzisse"~1'`) or `AND`/`OR`
does not work. 
Also currently no preview can be shown when using fuzzy search.
