import { Channel, invoke } from '@tauri-apps/api/core';
import type { AssistantEvent } from '../assistant/events';
import { SpeechPlayer } from './player';
import { SpeechText } from './text';

export interface SpeechSettings {
  enabled: boolean;
  volume: number;
  reference: string;
  model: string;
  projectDir: string;
}
export const defaultSpeechSettings = (): SpeechSettings => ({
  enabled: false,
  volume: 0.8,
  reference: '',
  model: '0.6B',
  projectDir: 'D:\\Projects\\fast-qwen3-tts',
});
interface SpeechEvent {
  sessionId: string;
  requestId: string;
  segmentId: string;
  event: { type: string; pcm?: string; sample_rate?: number; message?: string };
}
interface Session {
  id: string;
  request: string;
  settings: SpeechSettings;
  text: SpeechText;
  queue: string[];
  finished: boolean;
  running: boolean;
  ready: Promise<void>;
}

export class SpeechController {
  private player = new SpeechPlayer();
  private session: Session | null = null;
  private timer: ReturnType<typeof setInterval>;
  constructor(private report: (label: string, active: boolean) => void) {
    this.timer = setInterval(() => {
      const s = this.session;
      if (!s) return;
      if (this.player.buffered > 0) this.report('正在朗读', true);
      else if (s.finished && !s.running && !s.queue.length) {
        this.stop();
      } else if (!s.running && !s.queue.length) this.report('等待回复文字', true);
    }, 150);
  }
  begin(settings: SpeechSettings, turnId: string) {
    this.stop();
    if (!settings.enabled) return;
    if (!settings.reference) {
      this.report('播报失败：请先选择参考声音', false);
      return;
    }
    const s: Session = {
      id: crypto.randomUUID(),
      request: turnId,
      settings: { ...settings },
      text: new SpeechText(),
      queue: [],
      finished: false,
      running: false,
      ready: Promise.resolve(),
    };
    this.session = s;
    this.report('等待回复文字', true);
    s.ready = this.player.start(settings.volume).catch((e) => {
      if (this.session === s) this.fail(e);
    });
  }
  event(event: AssistantEvent) {
    const s = this.session;
    if (!s || s.request !== event.turnId) return;
    if (event.type === 'messageStarted') s.text = new SpeechText();
    if (event.type === 'textDelta') s.queue.push(...s.text.append(event.text));
    if (event.type === 'messageCompleted') s.queue.push(...s.text.finish(event.content));
    if (event.type === 'turnCompleted') s.finished = true;
    if (event.type === 'turnCancelled' || event.type === 'turnFailed') {
      this.stop();
      return;
    }
    if (s.queue.length > 128 || s.queue.reduce((n, text) => n + text.length, 0) > 64000) {
      this.fail('待朗读内容超过限制，本轮文字继续生成');
      return;
    }
    void this.pump(s);
  }
  preview(settings: SpeechSettings) {
    const turnId = crypto.randomUUID();
    this.begin({ ...settings, enabled: true }, turnId);
    this.event({
      version: 1,
      conversationId: 'preview',
      turnId,
      sequence: 1,
      type: 'messageCompleted',
      messageId: 'preview',
      content: '你好，我是你的桌面助手。现在可以一边聊天，一边听我说话。',
    });
    this.event({
      version: 1,
      conversationId: 'preview',
      turnId,
      sequence: 2,
      type: 'turnCompleted',
      revision: 0,
    });
  }
  volume(volume: number) {
    this.player.setVolume(volume);
  }
  stop() {
    const s = this.session;
    this.session = null;
    this.player.stop();
    this.report('', false);
    if (s)
      void invoke('cancel_speech', { sessionId: s.id }).catch(() =>
        this.report('语音取消通知未送达，声音已停止', false),
      );
  }
  dispose() {
    this.stop();
    clearInterval(this.timer);
  }
  private fail(error: unknown) {
    this.stop();
    this.report(`播报失败：${String(error)}`, false);
  }
  private async pump(s: Session) {
    if (s.running || this.session !== s) return;
    s.running = true;
    try {
      await s.ready;
      while (this.session === s && s.queue.length) {
        while (this.player.buffered > 15 && this.session === s)
          await new Promise((r) => setTimeout(r, 100));
        if (this.session !== s) break;
        const text = s.queue.shift()!;
        const segmentId = crypto.randomUUID();
        const onEvent = new Channel<SpeechEvent>();
        onEvent.onmessage = (payload) => {
          if (
            this.session !== s ||
            payload.sessionId !== s.id ||
            payload.requestId !== s.request ||
            payload.segmentId !== segmentId
          )
            return;
          try {
            if (payload.event.type === 'audio') {
              this.player.append(payload.event.pcm!, payload.event.sample_rate!);
              this.report('正在朗读', true);
            } else if (payload.event.type === 'status' && !this.player.buffered)
              this.report(payload.event.message!, true);
          } catch (e) {
            this.fail(e);
          }
        };
        await invoke('speak_segment', {
          input: { sessionId: s.id, requestId: s.request, segmentId, text, settings: s.settings },
          onEvent,
        });
      }
    } catch (e) {
      if (this.session === s) this.fail(e);
    } finally {
      s.running = false;
    }
  }
}
