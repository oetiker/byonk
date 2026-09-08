// Byonk dev-mode editor: the 1:1 pixel lens, shared by the editor page and
// the pop-out preview window.
//
// This file was lifted out of dev.js unchanged so both pages can magnify the
// render without a second copy of the code. It is a classic script (no
// module), so `setupLens` and `updateLensImage` are plain globals for the
// page script that loads after it.
//
// Contract with the page: it must contain exactly one `<img id="display-image">`
// — the element the user hovers — and it must call `updateLensImage(url)`
// with the FULL-RESOLUTION image URL every time that image changes. The lens
// magnifies from its own off-screen copy at that URL, not from the on-screen
// element, which is usually scaled down to fit; a page that never calls
// `updateLensImage` gets a lens that stays invisible rather than a wrong one.
//
// The styling (`.lens`, `.lens.hidden`, `.lens canvas`) lives in dev.css.

"use strict";

let lensCanvas = null;
let lensCtx = null;
let fullResImage = null;
let lensElement = null;

function setupLens() {
  lensElement = document.createElement("div");
  lensElement.id = "lens";
  lensElement.className = "lens hidden";
  document.body.appendChild(lensElement);

  lensCanvas = document.createElement("canvas");
  lensCanvas.width = 200;
  lensCanvas.height = 200;
  lensElement.appendChild(lensCanvas);
  lensCtx = lensCanvas.getContext("2d");

  fullResImage = new Image();

  const img = document.getElementById("display-image");
  img.addEventListener("mouseenter", showLens);
  img.addEventListener("mouseleave", hideLens);
  img.addEventListener("mousemove", moveLens);
}

function updateLensImage(url) {
  if (fullResImage) {
    fullResImage.src = url;
  }
}

function showLens(e) {
  if (!fullResImage.complete || !fullResImage.naturalWidth) return;
  lensElement.classList.remove("hidden");
  moveLens(e);
}

function hideLens() {
  lensElement.classList.add("hidden");
}

function moveLens(e) {
  if (!fullResImage.complete || !fullResImage.naturalWidth) return;

  const img = document.getElementById("display-image");
  const rect = img.getBoundingClientRect();
  const lensSize = 200;
  const lensRadius = lensSize / 2;
  const magnification = 2;
  const sourceSize = lensSize / magnification;

  const lensX = e.clientX + 20;
  const lensY = e.clientY - lensSize - 10;
  lensElement.style.left = `${lensX}px`;
  lensElement.style.top = `${lensY}px`;

  const imgX = e.clientX - rect.left;
  const imgY = e.clientY - rect.top;

  const scaleX = fullResImage.naturalWidth / rect.width;
  const scaleY = fullResImage.naturalHeight / rect.height;

  const srcW = sourceSize * scaleX;
  const srcH = sourceSize * scaleY;
  const srcX = (imgX * scaleX) - (srcW / 2);
  const srcY = (imgY * scaleY) - (srcH / 2);

  lensCtx.clearRect(0, 0, lensSize, lensSize);
  lensCtx.save();
  lensCtx.beginPath();
  lensCtx.arc(lensRadius, lensRadius, lensRadius - 2, 0, Math.PI * 2);
  lensCtx.clip();
  lensCtx.fillStyle = "#fff";
  lensCtx.fillRect(0, 0, lensSize, lensSize);
  lensCtx.imageSmoothingEnabled = false;
  lensCtx.drawImage(fullResImage, srcX, srcY, srcW, srcH, 0, 0, lensSize, lensSize);
  lensCtx.restore();
}
