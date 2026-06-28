// Source: https://www.e-paper-display.com/news_detail/newsId%3D81.html
// E-paper display dithering tool - JavaScript reference implementation
// Retrieved: 2026-02-05
//
// Key differences from our eink-dither Rust crate:
// - Color matching: Euclidean distance in sRGB (not perceptual OKLab)
// - Error diffusion: operates in sRGB gamma space (not linear RGB)
// - No serpentine scanning (always left-to-right)
// - No error clamping
// - "Blue noise" is actually white noise (Math.random), not true blue noise
// - Atkinson: 7 neighbors with 1/8 weight each (6/8 = 75% propagation) — correct
// - Floyd-Steinberg: standard 7/16, 3/16, 5/16, 1/16 weights — correct

// === Image upload and canvas setup ===

const uploadInput = document.getElementById('upload');
const ditherOptions = document.getElementById('dither-options');
const downloadButton = document.getElementById('download');
const canvas = document.getElementById('canvas');
const ctx = canvas.getContext('2d');

uploadInput.addEventListener('change', (event) => {
  const file = event.target.files[0];
  if (file) {
    const reader = new FileReader();
    reader.onload = function(e) {
      const img = new Image();
      img.onload = function() {
        canvas.width = img.width;
        canvas.height = img.height;
        ctx.drawImage(img, 0, 0);
        applyDithering();
      }
      img.src = e.target.result;
    }
    reader.readAsDataURL(file);
  }
});

ditherOptions.addEventListener('change', applyDithering);

downloadButton.addEventListener('click', () => {
  const link = document.createElement('a');
  link.download = 'dithered_image.png';
  link.href = canvas.toDataURL();
  link.click();
});

// === Dithering dispatch ===

function applyDithering() {
  const option = ditherOptions.value;
  const colors = getColors(option);
  if (option === 'blue-noise-black-white') {
    applyBlueNoiseDithering(colors);
  } else if (option === 'floyd-steinberg') {
    applyFloydSteinbergDithering(colors);
  } else if (option === 'atkinson') {
    applyAtkinsonDithering(colors);
  } else {
    applyErrorDiffusionDithering(colors);
  }
}

// === Palette definitions ===

function getColors(option) {
  switch (option) {
    case 'black-white':
      return [
        { r: 0, g: 0, b: 0 },
        { r: 255, g: 255, b: 255 }
      ];
    case 'black-white-red':
      return [
        { r: 0, g: 0, b: 0 },
        { r: 255, g: 255, b: 255 },
        { r: 255, g: 0, b: 0 }
      ];
    case 'black-white-red-yellow':
      return [
        { r: 0, g: 0, b: 0 },
        { r: 255, g: 255, b: 255 },
        { r: 255, g: 0, b: 0 },
        { r: 255, g: 255, b: 0 }
      ];
    case '6-colors':
      return [
        { r: 0, g: 0, b: 0 },
        { r: 255, g: 255, b: 255 },
        { r: 255, g: 255, b: 0 },
        { r: 255, g: 0, b: 0 },
        { r: 0, g: 255, b: 0 },
        { r: 0, g: 0, b: 255 }
      ];
    case '4-gray':
      return [
        { r: 0, g: 0, b: 0 },
        { r: 85, g: 85, b: 85 },
        { r: 170, g: 170, b: 170 },
        { r: 255, g: 255, b: 255 }
      ];
    case '16-gray':
      return [
        { r: 0, g: 0, b: 0 },
        { r: 17, g: 17, b: 17 },
        { r: 34, g: 34, b: 34 },
        { r: 51, g: 51, b: 51 },
        { r: 68, g: 68, b: 68 },
        { r: 85, g: 85, b: 85 },
        { r: 102, g: 102, b: 102 },
        { r: 119, g: 119, b: 119 },
        { r: 136, g: 136, b: 136 },
        { r: 153, g: 153, b: 153 },
        { r: 170, g: 170, b: 170 },
        { r: 187, g: 187, b: 187 },
        { r: 204, g: 204, b: 204 },
        { r: 221, g: 221, b: 221 },
        { r: 238, g: 238, b: 238 },
        { r: 255, g: 255, b: 255 }
      ];
    case 'blue-noise-black-white':
      return [
        { r: 0, g: 0, b: 0 },
        { r: 255, g: 255, b: 255 }
      ];
    default:
      return [
        { r: 0, g: 0, b: 0 },
        { r: 255, g: 255, b: 255 }
      ];
  }
}

// === Color matching (Euclidean in sRGB — NOT perceptually correct) ===

function findNearestColor(pixel, colors) {
  let nearestColor = colors[0];
  let minDistance = colorDistance(pixel, colors[0]);
  for (let i = 1; i < colors.length; i++) {
    const distance = colorDistance(pixel, colors[i]);
    if (distance < minDistance) {
      minDistance = distance;
      nearestColor = colors[i];
    }
  }
  return nearestColor;
}

function colorDistance(c1, c2) {
  return Math.sqrt(
    Math.pow(c1.r - c2.r, 2) +
    Math.pow(c1.g - c2.g, 2) +
    Math.pow(c1.b - c2.b, 2)
  );
}

// === Generic error diffusion (Floyd-Steinberg weights) ===

function applyErrorDiffusionDithering(colors) {
  const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const pixels = imageData.data;
  for (let y = 0; y < canvas.height; y++) {
    for (let x = 0; x < canvas.width; x++) {
      const index = (y * canvas.width + x) * 4;
      const oldPixel = {
        r: pixels[index],
        g: pixels[index + 1],
        b: pixels[index + 2]
      };
      const newPixel = findNearestColor(oldPixel, colors);
      pixels[index] = newPixel.r;
      pixels[index + 1] = newPixel.g;
      pixels[index + 2] = newPixel.b;
      distributeError(pixels, canvas.width, canvas.height, x, y, oldPixel, newPixel);
    }
  }
  ctx.putImageData(imageData, 0, 0);
}

function distributeError(pixels, width, height, x, y, oldPixel, newPixel) {
  const quantError = {
    r: oldPixel.r - newPixel.r,
    g: oldPixel.g - newPixel.g,
    b: oldPixel.b - newPixel.b
  };
  const errorWeight1 = 7 / 16;
  const errorWeight2 = 3 / 16;
  const errorWeight3 = 5 / 16;
  const errorWeight4 = 1 / 16;

  if (x + 1 < width) {
    applyError(pixels, width, height, x + 1, y, quantError, errorWeight1);
  }
  if (x > 0 && y + 1 < height) {
    applyError(pixels, width, height, x - 1, y + 1, quantError, errorWeight2);
  }
  if (y + 1 < height) {
    applyError(pixels, width, height, x, y + 1, quantError, errorWeight3);
  }
  if (x + 1 < width && y + 1 < height) {
    applyError(pixels, width, height, x + 1, y + 1, quantError, errorWeight4);
  }
}

function applyError(pixels, width, height, x, y, quantError, weight) {
  const index = (y * width + x) * 4;
  pixels[index] = clamp(pixels[index] + quantError.r * weight);
  pixels[index + 1] = clamp(pixels[index + 1] + quantError.g * weight);
  pixels[index + 2] = clamp(pixels[index + 2] + quantError.b * weight);
}

// === Blue noise dithering (actually white noise — Math.random) ===

function applyBlueNoiseDithering(colors) {
  const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const pixels = imageData.data;
  const noise = generateBlueNoise(canvas.width, canvas.height);
  for (let y = 0; y < canvas.height; y++) {
    for (let x = 0; x < canvas.width; x++) {
      const index = (y * canvas.width + x) * 4;
      const threshold = noise[y][x];
      const grayscale = pixels[index] * 0.299 + pixels[index + 1] * 0.587 + pixels[index + 2] * 0.114;
      const newColor = grayscale + threshold > 255 ? 255 : 0;
      pixels[index] = newColor;
      pixels[index + 1] = newColor;
      pixels[index + 2] = newColor;
    }
  }
  ctx.putImageData(imageData, 0, 0);
}

function generateBlueNoise(width, height) {
  const noise = new Array(height);
  for (let y = 0; y < height; y++) {
    noise[y] = new Array(width);
    for (let x = 0; x < width; x++) {
      noise[y][x] = Math.random() * 255;
    }
  }
  return noise;
}

// === Floyd-Steinberg dithering ===

function applyFloydSteinbergDithering(colors) {
  const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const pixels = imageData.data;
  for (let y = 0; y < canvas.height; y++) {
    for (let x = 0; x < canvas.width; x++) {
      const index = (y * canvas.width + x) * 4;
      const oldPixel = {
        r: pixels[index],
        g: pixels[index + 1],
        b: pixels[index + 2]
      };
      const newPixel = findNearestColor(oldPixel, colors);
      pixels[index] = newPixel.r;
      pixels[index + 1] = newPixel.g;
      pixels[index + 2] = newPixel.b;
      const quantErrorR = oldPixel.r - newPixel.r;
      const quantErrorG = oldPixel.g - newPixel.g;
      const quantErrorB = oldPixel.b - newPixel.b;
      distributeFloydSteinbergError(pixels, canvas.width, canvas.height, x, y, quantErrorR, quantErrorG, quantErrorB);
    }
  }
  ctx.putImageData(imageData, 0, 0);
}

function distributeFloydSteinbergError(pixels, width, height, x, y, quantErrorR, quantErrorG, quantErrorB) {
  const errorWeight1 = 7 / 16;
  const errorWeight2 = 3 / 16;
  const errorWeight3 = 5 / 16;
  const errorWeight4 = 1 / 16;

  if (x + 1 < width) {
    applyFSError(pixels, width, height, x + 1, y, quantErrorR, quantErrorG, quantErrorB, errorWeight1);
  }
  if (x > 0 && y + 1 < height) {
    applyFSError(pixels, width, height, x - 1, y + 1, quantErrorR, quantErrorG, quantErrorB, errorWeight2);
  }
  if (y + 1 < height) {
    applyFSError(pixels, width, height, x, y + 1, quantErrorR, quantErrorG, quantErrorB, errorWeight3);
  }
  if (x + 1 < width && y + 1 < height) {
    applyFSError(pixels, width, height, x + 1, y + 1, quantErrorR, quantErrorG, quantErrorB, errorWeight4);
  }
}

function applyFSError(pixels, width, height, x, y, quantErrorR, quantErrorG, quantErrorB, weight) {
  const index = (y * width + x) * 4;
  pixels[index] = clamp(pixels[index] + quantErrorR * weight);
  pixels[index + 1] = clamp(pixels[index + 1] + quantErrorG * weight);
  pixels[index + 2] = clamp(pixels[index + 2] + quantErrorB * weight);
}

// === Atkinson dithering ===
// 7 neighbors, 1/8 weight each = 7/8 total (87.5% propagation)
// Note: this propagates 7/8, not 6/8 (75%) like the original Atkinson.
// The original Atkinson uses 6 neighbors. This implementation adds a 7th
// neighbor (x+2, y+1) which is non-standard.

function applyAtkinsonDithering(colors) {
  const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const pixels = imageData.data;
  for (let y = 0; y < canvas.height; y++) {
    for (let x = 0; x < canvas.width; x++) {
      const index = (y * canvas.width + x) * 4;
      const oldPixel = {
        r: pixels[index],
        g: pixels[index + 1],
        b: pixels[index + 2]
      };
      const newPixel = findNearestColor(oldPixel, colors);
      pixels[index] = newPixel.r;
      pixels[index + 1] = newPixel.g;
      pixels[index + 2] = newPixel.b;
      const quantErrorR = oldPixel.r - newPixel.r;
      const quantErrorG = oldPixel.g - newPixel.g;
      const quantErrorB = oldPixel.b - newPixel.b;
      distributeAtkinsonError(pixels, canvas.width, canvas.height, x, y, quantErrorR, quantErrorG, quantErrorB);
    }
  }
  ctx.putImageData(imageData, 0, 0);
}

function distributeAtkinsonError(pixels, width, height, x, y, quantErrorR, quantErrorG, quantErrorB) {
  const errorWeight1 = 1 / 8;
  const errorWeight2 = 1 / 8;
  const errorWeight3 = 1 / 8;
  const errorWeight4 = 1 / 8;
  const errorWeight5 = 1 / 8;
  const errorWeight6 = 1 / 8;
  const errorWeight7 = 1 / 8;

  if (x + 1 < width) {
    applyAtkinsonError(pixels, width, height, x + 1, y, quantErrorR, quantErrorG, quantErrorB, errorWeight1);
  }
  if (x + 2 < width) {
    applyAtkinsonError(pixels, width, height, x + 2, y, quantErrorR, quantErrorG, quantErrorB, errorWeight2);
  }
  if (x > 0 && y + 1 < height) {
    applyAtkinsonError(pixels, width, height, x - 1, y + 1, quantErrorR, quantErrorG, quantErrorB, errorWeight3);
  }
  if (y + 1 < height) {
    applyAtkinsonError(pixels, width, height, x, y + 1, quantErrorR, quantErrorG, quantErrorB, errorWeight4);
  }
  if (x + 1 < width && y + 1 < height) {
    applyAtkinsonError(pixels, width, height, x + 1, y + 1, quantErrorR, quantErrorG, quantErrorB, errorWeight5);
  }
  if (x + 2 < width && y + 1 < height) {
    applyAtkinsonError(pixels, width, height, x + 2, y + 1, quantErrorR, quantErrorG, quantErrorB, errorWeight6);
  }
  if (y + 2 < height) {
    applyAtkinsonError(pixels, width, height, x, y + 2, quantErrorR, quantErrorG, quantErrorB, errorWeight7);
  }
}

function applyAtkinsonError(pixels, width, height, x, y, quantErrorR, quantErrorG, quantErrorB, weight) {
  const index = (y * width + x) * 4;
  pixels[index] = clamp(pixels[index] + quantErrorR * weight);
  pixels[index + 1] = clamp(pixels[index + 1] + quantErrorG * weight);
  pixels[index + 2] = clamp(pixels[index + 2] + quantErrorB * weight);
}

// === Utility ===

function clamp(value) {
  return Math.max(0, Math.min(255, value));
}
