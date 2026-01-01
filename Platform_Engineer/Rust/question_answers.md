
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

It depends on where this data is coming from. If it is coming from disk or over the network, it's very 
likely the compactness is improving the speed of deserializing significantly. But if the data was already 
loaded in RAM, then I'd forgo the bitpacking (especially the "ultrapacking") and keep things in byte/word 
aligned sections.

SIMD packing/unpacking would be good too, but that's assuming you have a good bit of integers/strings/etc. 
plus SIMD is rarely easy to maintain (maybe once we get portable-simd...).

Mainly, would need to profile/benchmark all of this, assumptions usually have to get thrown out of the 
window.

4. How would you modify your code to support direct access? In other words, could you read a single property without deserializing the entire file?

Header metadata bytes marking the start of each property described as a bit index. Could easily delta 
encode this too since it'd just be an monotonically increasing number, which conveniently gives us a length 
of the property as well if you just read the next property start, so we could offset the costs of this by 
removing the length headers in integers/strings.

For integers/booleans/arrays, storing that bit index is enough, but for the packed strings we'd need a bit index + index in the string bundle.

5. How would you extend your solution to further property types and increasing levels of nesting?

It depends on the kind of property types and what the requirements of extending it are.
Would we need to migrate a lot of existing metadata? Or is this all more transitory data.

If we don't need to migrate, then I'd just have a suggestion of keeping the more size bounded data near the front and the unbounded data near the end. If we *do* need migrations then I'd just append the new type at the end of the format and things should work about the same.

Nesting already works, these structures are all just flattened into the property pools.
