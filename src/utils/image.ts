/**
 * 图片处理工具
 * 用于图片压缩、格式转换等
 */

export interface ProcessedImage {
  dataUrl: string;
  width: number;
  height: number;
  size: number;
  format: string;
}

/**
 * 压缩图片
 */
export async function compressImage(
  file: File,
  maxWidth = 1920,
  maxHeight = 1080,
  quality = 0.85
): Promise<ProcessedImage> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = (e) => {
      const img = new Image();
      img.onload = () => {
        // 创建 canvas
        const canvas = document.createElement('canvas');
        let width = img.width;
        let height = img.height;

        // 计算缩放比例
        if (width > maxWidth || height > maxHeight) {
          const ratio = Math.min(maxWidth / width, maxHeight / height);
          width = Math.floor(width * ratio);
          height = Math.floor(height * ratio);
        }

        canvas.width = width;
        canvas.height = height;

        // 绘制图片
        const ctx = canvas.getContext('2d');
        if (!ctx) {
          reject(new Error('无法获取 canvas context'));
          return;
        }

        ctx.drawImage(img, 0, 0, width, height);

        // 导出为压缩后的图片
        canvas.toBlob(
          (blob) => {
            if (!blob) {
              reject(new Error('图片压缩失败'));
              return;
            }

            const reader = new FileReader();
            reader.onload = (e) => {
              const dataUrl = e.target?.result as string;
              resolve({
                dataUrl,
                width,
                height,
                size: blob.size,
                format: blob.type,
              });
            };
            reader.onerror = () => reject(new Error('读取压缩后的图片失败'));
            reader.readAsDataURL(blob);
          },
          'image/jpeg',
          quality
        );
      };

      img.onerror = () => reject(new Error('加载图片失败'));
      img.src = e.target?.result as string;
    };

    reader.onerror = () => reject(new Error('读取文件失败'));
    reader.readAsDataURL(file);
  });
}

/**
 * 读取图片为 base64
 */
export function readImageAsBase64(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = (e) => {
      const dataUrl = e.target?.result as string;
      // 移除 data:image/xxx;base64, 前缀
      const base64 = dataUrl.split(',')[1];
      resolve(base64);
    };

    reader.onerror = () => reject(new Error('读取文件失败'));
    reader.readAsDataURL(file);
  });
}

/**
 * 获取图片尺寸
 */
export function getImageDimensions(file: File): Promise<{ width: number; height: number }> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = (e) => {
      const img = new Image();
      img.onload = () => {
        resolve({
          width: img.width,
          height: img.height,
        });
      };

      img.onerror = () => reject(new Error('加载图片失败'));
      img.src = e.target?.result as string;
    };

    reader.onerror = () => reject(new Error('读取文件失败'));
    reader.readAsDataURL(file);
  });
}

/**
 * 格式化文件大小
 */
export function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/**
 * 检查文件是否为图片
 */
export function isImageFile(file: File): boolean {
  return file.type.startsWith('image/');
}

/**
 * 检查图片大小是否在限制内
 */
export function checkImageSize(file: File, maxSizeMB = 5): boolean {
  const maxSize = maxSizeMB * 1024 * 1024;
  return file.size <= maxSize;
}

/**
 * 从 data URL 获取 MIME 类型
 */
export function getMimeTypeFromDataUrl(dataUrl: string): string {
  const match = dataUrl.match(/^data:([^;]+);base64,/);
  return match ? match[1] : 'image/jpeg';
}

/**
 * 生成图片缩略图
 */
export async function generateThumbnail(
  file: File,
  size = 200
): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();

    reader.onload = (e) => {
      const img = new Image();
      img.onload = () => {
        const canvas = document.createElement('canvas');
        let width = img.width;
        let height = img.height;

        // 计算缩放比例
        if (width > height) {
          if (width > size) {
            height = Math.floor((height * size) / width);
            width = size;
          }
        } else {
          if (height > size) {
            width = Math.floor((width * size) / height);
            height = size;
          }
        }

        canvas.width = width;
        canvas.height = height;

        const ctx = canvas.getContext('2d');
        if (!ctx) {
          reject(new Error('无法获取 canvas context'));
          return;
        }

        ctx.drawImage(img, 0, 0, width, height);

        // 导出缩略图
        const thumbnail = canvas.toDataURL('image/jpeg', 0.7);
        resolve(thumbnail);
      };

      img.onerror = () => reject(new Error('加载图片失败'));
      img.src = e.target?.result as string;
    };

    reader.onerror = () => reject(new Error('读取文件失败'));
    reader.readAsDataURL(file);
  });
}

/**
 * 验证图片文件
 */
export function validateImageFile(file: File): { valid: boolean; error?: string } {
  // 检查文件类型
  if (!isImageFile(file)) {
    return { valid: false, error: '不是有效的图片文件' };
  }

  // 检查文件大小
  if (!checkImageSize(file)) {
    return { valid: false, error: '图片大小不能超过 5MB' };
  }

  // 检查文件扩展名
  const validExtensions = ['.jpg', '.jpeg', '.png', '.gif', '.webp', '.bmp'];
  const extension = '.' + file.name.split('.').pop()?.toLowerCase();
  if (!validExtensions.includes(extension)) {
    return { valid: false, error: '不支持的图片格式' };
  }

  return { valid: true };
}
