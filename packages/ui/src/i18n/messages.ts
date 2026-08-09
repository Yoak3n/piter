/**
 * Shared locale messages for both frontends (chat + admin).
 * 按 语言×领域 分文件：messages/{en,zh}/{common,chat,admin}.ts，
 * 本文件仅做合并导出（保持 `messages` 导出名与消费方不变）。
 */
import enCommon from "./messages/en/common";
import enChat from "./messages/en/chat";
import enAdmin from "./messages/en/admin";
import zhCommon from "./messages/zh/common";
import zhChat from "./messages/zh/chat";
import zhAdmin from "./messages/zh/admin";

export const messages = {
  en: {
    common: enCommon,
    chat: enChat,
    admin: enAdmin,
  },
  zh: {
    common: zhCommon,
    chat: zhChat,
    admin: zhAdmin,
  },
} as const;
