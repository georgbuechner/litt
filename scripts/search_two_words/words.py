import json 
from subprocess import PIPE, run

with open("two_words.json") as json_data:
    data = json.load(json_data)

for query in data: 
    print(query)
    w1 = query["word1"]
    w2 = query["word2"]
    
    command = ["litt", "two-words", f"'\"{w1} {w2}\"~0'"]
    res = run(command, stdin=PIPE, stdout=PIPE, stderr=PIPE, universal_newlines=True)
    rc = res.stdout
    print("OUTPUT: ", rc)
    if "[1]" not in rc: 
        print("NOT FOUND!!")
        x = input()

    
