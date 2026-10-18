#import <AppKit/AppKit.h>
#import <AVFoundation/AVFoundation.h>
#import <ScreenCaptureKit/ScreenCaptureKit.h>
#import <CoreMedia/CoreMedia.h>
#include <stdbool.h>

typedef void (*SamplesCallback)(void *, const float *, size_t);
typedef void (*ErrorCallback)(void *);

@interface DLLightAudio : NSObject <SCStreamOutput, SCStreamDelegate>
@property(nonatomic, strong) SCStream *stream;
@property(nonatomic, strong) dispatch_queue_t queue;
@property(nonatomic) SamplesCallback samples;
@property(nonatomic) ErrorCallback failure;
@property(nonatomic) void *context;
@end
@implementation DLLightAudio
- (void)stream:(SCStream *)stream didStopWithError:(NSError *)error {
    // Serialize errors with sample delivery so stop can drain both callbacks.
    dispatch_async(self.queue, ^{ if (self.failure) self.failure(self.context); });
}
- (void)stream:(SCStream *)stream didOutputSampleBuffer:(CMSampleBufferRef)sample ofType:(SCStreamOutputType)type {
    if (!self.samples || type != SCStreamOutputTypeAudio || !CMSampleBufferDataIsReady(sample)) return;
    CMAudioFormatDescriptionRef description = CMSampleBufferGetFormatDescription(sample);
    const AudioStreamBasicDescription *format = CMAudioFormatDescriptionGetStreamBasicDescription(description);
    if (!format || !(format->mFormatFlags & kAudioFormatFlagIsFloat) || format->mBitsPerChannel != 32) return;
    size_t required = 0;
    CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sample, &required, NULL, 0, NULL, NULL, 0, NULL);
    AudioBufferList *list = malloc(required);
    if (!list) return;
    CMBlockBufferRef retained = NULL;
    OSStatus status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(sample, NULL, list, required, NULL, NULL, kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment, &retained);
    if (status == noErr && list->mNumberBuffers) {
        // ScreenCaptureKit normally produces planar float stereo. Use one
        // channel; support interleaved buffers by stepping over other channels.
        AudioBuffer buffer = list->mBuffers[0];
        size_t channels = MAX(1, buffer.mNumberChannels);
        size_t frames = buffer.mDataByteSize / sizeof(float) / channels;
        const float *data = buffer.mData;
        if (channels == 1) self.samples(self.context, data, frames);
        else {
            float *mono = malloc(frames * sizeof(float));
            if (mono) { for (size_t i=0;i<frames;i++) mono[i]=data[i*channels]; self.samples(self.context,mono,frames); free(mono); }
        }
    }
    if (retained) CFRelease(retained);
    free(list);
}
@end

void *dl_audio_start(SamplesCallback samples, ErrorCallback failure, void *context, char *message, size_t length) {
    @autoreleasepool {
        dispatch_semaphore_t ready = dispatch_semaphore_create(0);
        __block SCShareableContent *content = nil;
        __block NSError *error = nil;
        [SCShareableContent getShareableContentWithCompletionHandler:^(SCShareableContent *value, NSError *err) { content=value; error=err; dispatch_semaphore_signal(ready); }];
        dispatch_semaphore_wait(ready, DISPATCH_TIME_FOREVER);
        if (!content.displays.count || error) { snprintf(message,length,"%s",error.localizedDescription.UTF8String ?: "No display available for system audio. Check Screen & System Audio Recording permission."); return NULL; }
        SCContentFilter *filter = [[SCContentFilter alloc] initWithDisplay:content.displays.firstObject excludingWindows:@[]];
        SCStreamConfiguration *config = [SCStreamConfiguration new];
        config.width=2; config.height=2; config.minimumFrameInterval=CMTimeMake(1,1);
        config.capturesAudio=YES; config.sampleRate=48000; config.channelCount=2;
        config.excludesCurrentProcessAudio=YES;
        DLLightAudio *handler=[DLLightAudio new];
        handler.samples=samples; handler.failure=failure; handler.context=context;
        handler.queue=dispatch_queue_create("com.dynamiclight.audio", DISPATCH_QUEUE_SERIAL);
        handler.stream=[[SCStream alloc] initWithFilter:filter configuration:config delegate:handler];
        if (![handler.stream addStreamOutput:handler type:SCStreamOutputTypeAudio sampleHandlerQueue:handler.queue error:&error]) {
            snprintf(message,length,"%s",error.localizedDescription.UTF8String); return NULL;
        }
        [handler.stream startCaptureWithCompletionHandler:^(NSError *err) {error=err; dispatch_semaphore_signal(ready);}];
        dispatch_semaphore_wait(ready, DISPATCH_TIME_FOREVER);
        if (error) {
            [handler.stream removeStreamOutput:handler type:SCStreamOutputTypeAudio error:nil];
            dispatch_sync(handler.queue, ^{handler.samples=NULL;handler.failure=NULL;});
            snprintf(message,length,"%s",error.localizedDescription.UTF8String); return NULL;
        }
        return (__bridge_retained void *)handler;
    }
}
bool dl_audio_stop(void *value) {
    @autoreleasepool {
        DLLightAudio *handler=(__bridge DLLightAudio *)value;
        dispatch_semaphore_t done=dispatch_semaphore_create(0);
        [handler.stream stopCaptureWithCompletionHandler:^(NSError *err) {dispatch_semaphore_signal(done);}];
        dispatch_semaphore_wait(done,DISPATCH_TIME_FOREVER);
        // Even when the stream has already stopped with an error, detach and
        // drain callbacks before releasing the Rust context.
        [handler.stream removeStreamOutput:handler type:SCStreamOutputTypeAudio error:nil];
        dispatch_sync(handler.queue, ^{handler.samples=NULL;handler.failure=NULL;});
        handler.stream=nil;
        (void)CFBridgingRelease(value);
        return true;
    }
}
bool dl_microphone_permission(void) {
    AVAuthorizationStatus status=[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio];
    if (status==AVAuthorizationStatusAuthorized) return true;
    if (status!=AVAuthorizationStatusNotDetermined) return false;
    dispatch_semaphore_t ready=dispatch_semaphore_create(0); __block BOOL allowed=NO;
    [AVCaptureDevice requestAccessForMediaType:AVMediaTypeAudio completionHandler:^(BOOL result) {allowed=result; dispatch_semaphore_signal(ready);}];
    dispatch_semaphore_wait(ready,DISPATCH_TIME_FOREVER);return allowed;
}
void dl_watch_power(void (*callback)(void *,bool), void *context) {
    NSNotificationCenter *center=NSWorkspace.sharedWorkspace.notificationCenter;
    // Process-lifetime observers; their Rust context has the same lifetime.
    [center addObserverForName:NSWorkspaceWillSleepNotification object:nil queue:nil usingBlock:^(NSNotification *note) {callback(context,true);}];
    [center addObserverForName:NSWorkspaceDidWakeNotification object:nil queue:nil usingBlock:^(NSNotification *note) {callback(context,false);}];
}
