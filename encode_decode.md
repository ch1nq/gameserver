# Comparisson of encoding/decoding solutions

|Python package|Rust crate|Compression|Other|Updates / second|
|--|--|--|--|--|
|pydantic|serde_json|-| |~300|
|cattrs|serde_json|-| |~350|
|cattrs|serde_json|zlib| |~370|
|cattrs|serde_json|smaz| |~375|
|cattrs|serde_json|smaz|"event_type" -> "e"|~400|
|cattrs|serde_json|-|"event_type" -> "e"|~420|
|cattrs|serde_json|-|"event_type" -> "e" + game diff |~520|
