// Подготовка портрета ассистента: квадратная обрезка по центру и сжатие до 320×320.
const TARGET_SIZE = 320;
const MAX_DATA_URL_LEN = 160_000;

function loadImage(url) {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("Не удалось прочитать изображение"));
    img.src = url;
  });
}

export async function resizeAvatarFile(file) {
  if (!file || !file.type?.startsWith("image/")) {
    throw new Error("Выберите файл изображения (JPG, PNG или WebP)");
  }
  const objectUrl = URL.createObjectURL(file);
  try {
    const img = await loadImage(objectUrl);
    const side = Math.min(img.naturalWidth || img.width, img.naturalHeight || img.height);
    if (!side) throw new Error("Пустое изображение");
    const sx = Math.floor(((img.naturalWidth || img.width) - side) / 2);
    const sy = Math.floor(((img.naturalHeight || img.height) - side) / 2);

    const canvas = document.createElement("canvas");
    canvas.width = TARGET_SIZE;
    canvas.height = TARGET_SIZE;
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Canvas недоступен");
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, sx, sy, side, side, 0, 0, TARGET_SIZE, TARGET_SIZE);

    for (const [mime, quality] of [
      ["image/webp", 0.84],
      ["image/webp", 0.72],
      ["image/jpeg", 0.82],
      ["image/jpeg", 0.68],
    ]) {
      const dataUrl = canvas.toDataURL(mime, quality);
      if (dataUrl.startsWith(`data:${mime};base64,`) && dataUrl.length <= MAX_DATA_URL_LEN) {
        return dataUrl;
      }
    }
    throw new Error("Изображение слишком сложное для сжатия — попробуйте другое");
  } finally {
    URL.revokeObjectURL(objectUrl);
  }
}
