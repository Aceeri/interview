

## Assumptions
- Focus on smaller configs, avoid bloating beyond initial size with metadata.
- Source of data is verified outside of serializing/deserializing, but invalid properties should still be caught in case of any form of data corruption.
- Version id assignment of schemas is handled by the user/tooling.
- Integers are likely small.
- Strings are likely ascii and english, but still have a good chance of containing many 'uncommon' characters. And in those scenarios the fields containing uncommon characters are likely to have similar characters.

## Solutions
Each property is put into a pool of the same data type and serialized together. These properties are order-dependent on when they were written and being read from.

Integers are compressed using a utf-8-esque prefix scheme, e.g. 0, 10, 110, 1110
These prefixes are used as indices into a bit width LUT, which is biased towards smaller values.

Booleans are bitpacked into a simple bitset. Randomness of these probably approaches 50-50 for configs, so this is probably about as compressed as we will get it. A single bit header + RLE encoding might give you some gains, but is likely to just bloat too much on metadata since you'd consistently need multiple sequences of the same value for it to be worth it.

Strings are the most interesting part. Libraries like zstd and such beat out on larger strings/larger datasets, but we have two approaches that beat them out fairly consistently on smaller strings:
- Abstracting over things like linearizing into an multi-dimensional array which lets use utilize more of the wasted space by combining values together based on their maximum value.
- Huffman encoding with a decent table for the dataset.

Huffman encoding prioritizes encoding common data more concisely at the expense of uncommon data becoming more bloated, while the linearization/ultrapacking encodes data down based on the total number of possible values meaning it performs better in more random datasets (though still in the expected ranges).

Combining these using a single bit header per string means we get the best of both worlds for minimal cost. Data that is just english phrases will be compressed much better by the huffman encoder, while trickier data from EXIF or program configurations will be compressed better by the ultrapacker.

Arrays are encoded as an integer for length and a list of property types. Past that the compression comes from the pre-existing int/bool/str compression. Property types currently fit nicely into 2 bits and utilize all 4 values, though there might be some room there for compression it seems minimal and noisy.

## Questions

1. How would you think about code maintainability here. Don't write any tests, but give some ideas for how a test suite could confirm the correctness of this code at compile time or test time.

The bitpacking/type compression is one of the more logic heavy parts of the code and compile time checks
would probably be more trouble than its worth.
- Fuzzy testing would provide a lot more certainty that any scenario works correctly.
- Making sure the boundaries between types are defined and never run over. This is mainly an issue with the dynamically sized types.
- Keeping the dynamic types like strings at the end of the format and maybe use a sentinel EOF value as a runtime safeguard.

A lot of this compactness also depends heavily on ordering of fields. A checksum test to make sure someone
didn't accidentally modify a schema without changed the protocol version would be good. Otherwise I'd just
give up the bits and say we have a 32 bit checksum on each message to avoid headaches.

2. What if you wanted to make the schema self-describing. How would you change your implementation?

Property names could be rolled into the same area as properties themselves. So the header format might look something like:
```rust
// while outside of a `PropertyType::Array` we expect a name => data mapping:
(PropertyType, PropertyType)
```
Alternatively we could just always assume the property name is a `PropertyType::String` and only mark a single `PropertyType`. We'd just take from the strings as we do with the normal data.

3. What if you wanted to prioritize speed rather than compactness? What if you wanted to prioritize both equally?

It depends on where this data is coming from: if it is coming from disk or over the network, it's very 
likely the compactness is improving the speed of deserializing significantly and ideally this is is turned 
into a streaming decoder. 
Otherwise:
- No bitpacking, keep byte alignments (especially the "ultrapacking")
- Integers should lose their variable-ness, though daniel lemire's stuff *may* work decently. Unsure at this
  small of configurations and would prefer to profile this.
- Store raw utf-8 bytes. If I was looking for both speed/compactness, I'd aim for just the huffman encoder.
- SIMD packing/unpacking might work well too, but SIMD is rarely easy to maintain (maybe once we get portable-simd...).

Mainly, would prefer to profile/benchmark a couple of variations on this, but these are my base assumptions.

4. How would you modify your code to support direct access? In other words, could you read a single property without deserializing the entire file?

Header metadata bytes marking the start of each property described as a bit index. Could easily delta 
encode this too since it'd just be an monotonically increasing number, which conveniently gives us a length 
of the property as well if you just read the next property start, so we could offset the costs of this by 
removing the length headers in integers/strings.

For integers/booleans/arrays, storing that bit index is enough, but for the packed strings we'd need a bit index + bundle index.

5. How would you extend your solution to further property types and increasing levels of nesting?

Putting new property types at the end of the format should get us most of the way, but some form of versioning schema would need to be thought up. Either by having the user/tooling manually modify a versioning byte or by checksumming each distinct type written to the serializer (outside of arrays, as they are dynamic).
Past that array property tags would need to be increased by another bit or more depending on the amount of property types needed.

Nesting already works pretty trivially, these structures are all just flattened into the property pools.
