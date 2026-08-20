# Decoded-image correction research

This note records the research and approved direction for white balance, local contrast, and noise reduction on already-rendered PNG and JPEG inputs. These controls remain separate from the future RAW-specific implementations.

## Reference findings

darktable's `src/iop/temperature.c` ultimately applies channel multipliers. Its temperature, tint, camera preset, and sensor-aware paths derive those multipliers from information which a rendered JPEG or PNG generally no longer retains.

RawTherapee likewise provides camera, automatic, spot, temperature, and tint white-balance machinery. Reproducing that interface for decoded inputs would imply physical meaning which those pixels cannot support.

RawTherapee's `rtengine/iplocalcontrast.cc` implements a particularly clear local-contrast reference: blur Lab lightness, calculate the lightness detail, scale it, optionally weight positive and negative detail separately, and add it back. Its default parameters use an 80-pixel radius and 0.2 amount.

darktable's `src/iop/bilat.c` provides bilateral-grid and local-Laplacian local contrast. They offer stronger edge awareness, but add substantially more processing, memory, tiling, and parameter complexity than the first FocalCore implementation needs.

darktable's `src/iop/denoiseprofile.c` uses camera/ISO Poissonian-Gaussian noise profiles with wavelet and non-local-means modes. RawTherapee's directional-pyramid denoiser is similarly sophisticated. Both demonstrate the importance of separating luminance and chromatic noise, but neither profile model transfers honestly to an image already processed by a camera or another editor.

## FocalPlane direction

- Warmth and Tint derive opponent-channel gains in linear Adobe RGB. They are deliberately not expressed as Kelvin.
- Local Contrast uses perceptual Adobe RGB luma and a fast three-box approximation of a Gaussian base. Amount reinjects or suppresses the resulting detail; Radius is expressed in pixels of the image being rendered.
- Noise Reduction uses a small edge-aware neighbourhood guided by perceptual Adobe RGB luma. Luminance and Colour strengths blend the filtered lightness and chroma independently.
- Neutral parameters are identities, parameters are validated before pixels are processed, and spatial loops contain cooperative cancellation checks.
- Future RAW white balance and profiled RAW denoising will be distinct modules or pipeline-version semantics. The decoded-image denoiser remains available for non-RAW files.

Human visual testing should check skin tones, neutral objects, saturated edges, fine foliage, low-light chroma speckle, flat skies, halos around high-contrast boundaries, and preview responsiveness at large local-contrast radii.
