import { Injectable, signal, computed, inject } from '@angular/core';
import { DOCUMENT } from '@angular/common';

export type SupportedLang = 'en' | 'zh';

interface TranslationResources {
  [lang: string]: {
    translation: Record<string, string>;
    common: Record<string, string>;
  };
}

const resources: TranslationResources = {
  en: {
    translation: {
      overview: 'Overview',
      trace: 'Trace',
      node: 'Node',
      app: 'App',
      notify: 'Notification',
      setting: 'Setting',
    },
    common: {
      ok: 'OK',
      cancel: 'Cancel',
      confirm: 'Confirm',
      close: 'Close',
      save: 'Save',
      delete: 'Delete',
      edit: 'Edit',
      add: 'Add',
      search: 'Search',
      loading: 'Loading',
      noData: 'No Data',
      loadMore: 'Load More',
    },
  },
  zh: {
    translation: {
      overview: '概览',
      trace: '追踪',
      node: '节点',
      app: '应用',
      notify: '通知',
      setting: '设置',
    },
    common: {
      ok: '确定',
      cancel: '取消',
      confirm: '确认',
      close: '关闭',
      save: '保存',
      delete: '删除',
      edit: '编辑',
      add: '添加',
      search: '搜索',
      loading: '加载中',
      noData: '暂无数据',
      loadMore: '加载更多',
    },
  },
};

@Injectable({ providedIn: 'root' })
export class TranslationService {
  private readonly document = inject(DOCUMENT);
  private readonly _lang = signal<SupportedLang>('en');

  readonly lang = this._lang.asReadonly();

  readonly isZh = computed(() => this._lang() === 'zh');

  constructor() {
    // Detect browser language
    const browserLang = navigator.language?.split('-')[0];
    if (browserLang === 'zh') {
      this.setLang('zh');
    }
  }

  setLang(lang: SupportedLang): void {
    this._lang.set(lang);
    this.document.documentElement.lang = lang;
  }

  t(key: string, ns: 'translation' | 'common' = 'translation'): string {
    const lang = this._lang();
    return resources[lang]?.[ns]?.[key] ?? key;
  }

  /** Usage: {{ 'common:search' | t }} */
  translate(key: string): string {
    const parts = key.split(':');
    if (parts.length === 2) {
      return this.t(parts[1], parts[0] as 'common');
    }
    return this.t(parts[0]);
  }
}
