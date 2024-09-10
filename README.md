# soundkit-flac



### notes on alignment

FLAC's internal representation: FLAC internally uses 32-bit signed integers to represent samples, regardless of the actual bit depth of the audio.
Right-justification requirement: FLAC expects these 32-bit integers to be right-justified. This means that for bit depths less than 32, the significant bits should be in the least significant bits of the 32-bit integer.
24-bit audio peculiarity: When we read 24-bit audio data into a 32-bit integer in most systems, it typically ends up left-justified (in the most significant bits). This is why we need to shift it right by 8 bits for FLAC.

This shift is specific to FLAC and isn't typically necessary when working with 24-bit audio in other contexts. It's a quirk of the FLAC format and its internal representation of audio samples.
