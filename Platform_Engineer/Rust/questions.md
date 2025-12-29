
## Questions

1. How would you think about code maintainability here. Don't write any tests, but give some ideas for how a test suite could confirm the correctness of this code at compile time or test time.

The bitpacking/type compression is one of the more logic heavy parts of the code, making types to safeguard
the logic seems like it'd create more technical debt than save, so I'd want to make sure there are a couple
of clear tests for:
- Making sure the boundaries between types are defined and never run over. This is mainly an issue with the dynamically sized types.
- Keeping the dynamic types like strings at the end of the format and maybe use a sentinel EOF value as a runtime safeguard.

A lot of this compactness also depends heavily on ordering of fields. It'd make sense to have a
hash at the start of the message to verify the schema matching up before we deserialize. Or alternatively
setting up a CI test per config struct to verify the schema matches up with the previously written one
minus suffixed additions (with some ways to "force" your way out). I left it out for the sake of 
compactness, but in practice I'd keep it because it'd reduce a lot of headaches.

2. What if you wanted to make the schema self-describing. How would you change your implementation?
Mainly I'd think about just adding a header to the beginning, the main thing that is implied is the ordering of the properties and then property naming.

Property names could be rolled into the same area as properties themselves. So the header format might look something like:
```rust
enum PropertyType {
    String,
    Int,
    Bool,
    ArrayStart, // basically a stack start/end, values inside this are assumed to not have a property name.
    ArrayEnd,
}

// while outside of an `ArrayStart/End` marked area:
(PropertyType, PropertyType) // marks the name => data, ordering still matters in packing/unpacking
// alternatively we could just always assume the property name is a `String` and only mark a single `PropertyType`.
```

3. What if you wanted to prioritize speed rather than compactness? What if you wanted to prioritize both equally?

It depends on where this data is coming from. If it is coming from disk or over the network, it's very 
likely the compactness is improving the speed of deserializing significantly. But if the data was already 
loaded in RAM, then I'd probably forgoe the bitpacking and keep things in byte/word aligned sections. This 
is probably as far as I'd go for balancing speed and compactness. 

If we could force some assumptions about string length, we could also remove any chance of allocations during deserialization.

I'd also assume that unpacking integers into words would be faster. SIMD unpacking would be good too, but that's assuming you have a good bit of integers/strings/etc. plus SIMD is rarely easy to maintain (maybe once we get portable-simd in stable...).

Would need to profile all of this, most assumptions usually have to get thrown out of the window constantly when dealing with this small of data changes.

4. How would you modify your code to support direct access? In other words, could you read a single property without deserializing the entire file?

Header metadata bytes marking the start of each property described at a bit index. Could easily delta 
encode this too since it'd just be an monotonically increasing number, which conveniently gives us a length 
of the property as well if you just read the next property start too. After that is set up it should be 
trivial to read a single property just like the non-marked version does.

5. How would you extend your solution to further property types and increasing levels of nesting?

It depends on the kind of property types and what the requirements of extending it are.
Would we need to migrate a lot of existing metadata? Or is this all more transitory data.

If we don't need to migrate, then I'd just have a suggestion of keeping the more size bounded data near the front and the unbounded data near the end. If we *do* need migrations then I'd just append the new type at the end of the format and things should work the same.

I'd still suggest keeping data of types together, it enables much better compression internally & it should lead to better compression if thrown through something like zstd/lzf/whichever since it's effectively doing something like BWT preprocessing.

Nesting could largely be handled the same way the arrays are currently handled: flattening the structure down to their properties.
